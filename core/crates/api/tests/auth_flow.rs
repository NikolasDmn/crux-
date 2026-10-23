//! Auth and account flows end to end on a real MySQL (spec 4.1, 13).

mod common;

use axum::http::StatusCode;
use crux_testkit::{call, outbox_topics, refresh_cookie, seed_invite, signup};
use serde_json::json;

fn signup_body(code: &str, name: &str) -> serde_json::Value {
    json!({
        "inviteCode": code, "email": format!("{name}@example.com"), "username": name,
        "firstName": "A", "lastName": "B", "password": "correct horse battery", "timeZone": "Europe/Amsterdam"
    })
}

#[tokio::test]
async fn signup_login_me_and_events() {
    let Some(h) = common::harness().await else { return };
    let (token, user, _) = signup(&h.app, &h.t.ctx, "nick").await;
    assert_eq!(user["username"], "nick");
    assert_eq!(user["tier"], "free");
    assert_eq!(user["emailVerified"], false);
    assert_eq!(user["id"].as_str().unwrap().len(), 26, "only PublicIds leave the service");

    let me = call(&h.app, "GET", "/me", Some(&token), None, &[]).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json["email"], "nick@example.com");

    let login =
        call(&h.app, "POST", "/auth/login", None, Some(json!({"login": "NICK@example.com", "password": "correct horse battery"})), &[])
            .await;
    assert_eq!(login.status, StatusCode::OK, "{}", login.json);

    let bad = call(&h.app, "POST", "/auth/login", None, Some(json!({"login": "nick", "password": "wrong password!"})), &[]).await;
    assert_eq!(bad.status, StatusCode::UNAUTHORIZED);
    let nobody = call(&h.app, "POST", "/auth/login", None, Some(json!({"login": "ghost", "password": "wrong password!"})), &[]).await;
    let strip = |mut v: serde_json::Value| {
        v["error"]["requestId"] = serde_json::Value::Null;
        v
    };
    assert_eq!(strip(nobody.json), strip(bad.json), "unknown user and wrong password look the same");

    // Signup wrote user.created and the verify-email request in the same transaction.
    assert_eq!(outbox_topics(&h.db.pool).await, ["user.created", "notify.requested"]);
    let tier_change: (String,) = sqlx::query_as("SELECT Reason FROM UserTierChange").fetch_one(&h.db.pool).await.unwrap();
    assert_eq!(tier_change.0, "signup");
    h.db.drop_db().await;
}

#[tokio::test]
async fn invite_rules() {
    let Some(h) = common::harness().await else { return };
    let r = call(&h.app, "POST", "/auth/signup", None, Some(signup_body("NOPE-NOPE", "nobody")), &[]).await;
    assert_eq!((r.status, r.code()), (StatusCode::FORBIDDEN, "invite_invalid"));

    let code = seed_invite(&h.t.ctx, 1).await;
    let ok = call(&h.app, "POST", "/auth/signup", None, Some(signup_body(&code.to_lowercase(), "first")), &[]).await;
    assert_eq!(ok.status, StatusCode::OK, "codes ignore case: {}", ok.json);
    let used = call(&h.app, "POST", "/auth/signup", None, Some(signup_body(&code, "second")), &[]).await;
    assert_eq!(used.code(), "invite_invalid");
    assert_eq!(used.json["error"]["details"]["reason"], "used_up");

    let code = seed_invite(&h.t.ctx, 5).await;
    let dup = call(&h.app, "POST", "/auth/signup", None, Some(signup_body(&code, "first")), &[]).await;
    assert_eq!(dup.status, StatusCode::UNPROCESSABLE_ENTITY);
    let fields: Vec<&str> =
        dup.json["error"]["details"]["fields"].as_array().unwrap().iter().map(|f| f["field"].as_str().unwrap()).collect();
    assert_eq!(fields, ["username"]);
    let uses: (i32,) = sqlx::query_as("SELECT Uses FROM Invite WHERE MaxUses = 5").fetch_one(&h.db.pool).await.unwrap();
    assert_eq!(uses.0, 0, "a failed signup doesn't burn an invite use");

    let weak = call(
        &h.app,
        "POST",
        "/auth/signup",
        None,
        Some(json!({
            "inviteCode": code, "email": "bad", "username": "u", "firstName": "", "lastName": "B",
            "password": "password123456", "timeZone": "Mars/Base"
        })),
        &[],
    )
    .await;
    let fields: Vec<&str> =
        weak.json["error"]["details"]["fields"].as_array().unwrap().iter().map(|f| f["field"].as_str().unwrap()).collect();
    assert_eq!(fields, ["email", "username", "firstName", "timeZone", "password"]);
    h.db.drop_db().await;
}

#[tokio::test]
async fn refresh_rotates_and_reuse_revokes_everything() {
    let Some(h) = common::harness().await else { return };
    let (_, _, cookie1) = signup(&h.app, &h.t.ctx, "rot").await;
    let cookie_hdr = |c: &str| format!("crux_refresh={c}");

    let r1 = call(&h.app, "POST", "/auth/refresh", None, None, &[("cookie", &cookie_hdr(&cookie1))]).await;
    assert_eq!(r1.status, StatusCode::OK);
    let cookie2 = refresh_cookie(&r1.headers).unwrap();
    assert_ne!(cookie1, cookie2, "the refresh token rotates on every use");

    // A second device's login is a separate session; reuse must revoke it too.
    let other = call(&h.app, "POST", "/auth/login", None, Some(json!({"login": "rot", "password": "correct horse battery"})), &[]).await;
    let other_cookie = refresh_cookie(&other.headers).unwrap();

    let replay = call(&h.app, "POST", "/auth/refresh", None, None, &[("cookie", &cookie_hdr(&cookie1))]).await;
    assert_eq!(replay.status, StatusCode::UNAUTHORIZED);
    for c in [&cookie2, &other_cookie] {
        let r = call(&h.app, "POST", "/auth/refresh", None, None, &[("cookie", &cookie_hdr(c))]).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "reuse revoked every session of the user");
    }
    h.db.drop_db().await;
}

#[tokio::test]
async fn logout_revokes_the_session() {
    let Some(h) = common::harness().await else { return };
    let (_, _, cookie) = signup(&h.app, &h.t.ctx, "bye").await;
    let hdr = format!("crux_refresh={cookie}");
    let out = call(&h.app, "POST", "/auth/logout", None, None, &[("cookie", &hdr)]).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    let r = call(&h.app, "POST", "/auth/refresh", None, None, &[("cookie", &hdr)]).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    h.db.drop_db().await;
}

#[tokio::test]
async fn login_is_rate_limited_per_account() {
    let Some(h) = common::harness().await else { return };
    signup(&h.app, &h.t.ctx, "slow").await;
    for _ in 0..5 {
        let r = call(&h.app, "POST", "/auth/login", None, Some(json!({"login": "slow", "password": "nope nope nope"})), &[]).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    }
    let r = call(&h.app, "POST", "/auth/login", None, Some(json!({"login": "slow", "password": "correct horse battery"})), &[]).await;
    assert_eq!((r.status, r.code()), (StatusCode::TOO_MANY_REQUESTS, "rate_limited"));
    assert!(r.headers.contains_key("retry-after"));
    h.db.drop_db().await;
}

#[tokio::test]
async fn patch_me_needs_if_match_and_detects_conflicts() {
    let Some(h) = common::harness().await else { return };
    let (token, user, _) = signup(&h.app, &h.t.ctx, "edit").await;
    let v = user["version"].as_i64().unwrap().to_string();

    let missing = call(&h.app, "PATCH", "/me", Some(&token), Some(json!({"firstName": "X"})), &[]).await;
    assert_eq!(missing.status, StatusCode::UNPROCESSABLE_ENTITY);

    let ok = call(&h.app, "PATCH", "/me", Some(&token), Some(json!({"firstName": "Nick", "timeZone": "UTC"})), &[("if-match", &v)]).await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.json);
    assert_eq!(ok.json["firstName"], "Nick");

    let stale = call(&h.app, "PATCH", "/me", Some(&token), Some(json!({"firstName": "Old"})), &[("if-match", &v)]).await;
    assert_eq!((stale.status, stale.code()), (StatusCode::CONFLICT, "conflict"));
    assert_eq!(stale.json["error"]["details"]["current"]["firstName"], "Nick", "conflict returns the current resource");

    let s = call(&h.app, "GET", "/me/settings", Some(&token), None, &[]).await;
    let sv = s.json["version"].as_i64().unwrap().to_string();
    let s2 = call(
        &h.app,
        "PATCH",
        "/me/settings",
        Some(&token),
        Some(json!({"flashNewPerDay": 10, "extra": {"dailyHours": 3}})),
        &[("if-match", &sv)],
    )
    .await;
    assert_eq!(s2.status, StatusCode::OK, "{}", s2.json);
    assert_eq!(s2.json["flashNewPerDay"], 10);
    assert_eq!(s2.json["flashRetention"], 0.9);
    h.db.drop_db().await;
}

#[tokio::test]
async fn verify_email_and_reset_password() {
    let Some(h) = common::harness().await else { return };
    let (token, user, _) = signup(&h.app, &h.t.ctx, "mail").await;
    let link: String = sqlx::query_scalar(
        "SELECT CAST(JSON_UNQUOTE(JSON_EXTRACT(Payload, '$.payload.link')) AS CHAR) FROM Outbox WHERE Topic = 'notify.requested'",
    )
    .fetch_one(&h.db.pool)
    .await
    .unwrap();
    let verify_token = link.split("token=").nth(1).unwrap();
    let r = call(&h.app, "POST", "/auth/verify-email/confirm", None, Some(json!({"token": verify_token})), &[]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["emailVerified"], true);
    let again = call(&h.app, "POST", "/auth/verify-email/confirm", None, Some(json!({"token": verify_token})), &[]).await;
    assert_eq!(again.status, StatusCode::UNPROCESSABLE_ENTITY, "one-time tokens are one-time");

    let f = call(&h.app, "POST", "/auth/password/forgot", None, Some(json!({"email": "mail@example.com"})), &[]).await;
    assert_eq!(f.status, StatusCode::ACCEPTED);
    let f2 = call(&h.app, "POST", "/auth/password/forgot", None, Some(json!({"email": "ghost@example.com"})), &[]).await;
    assert_eq!(f2.status, StatusCode::ACCEPTED, "no account enumeration");

    let link: String = sqlx::query_scalar("SELECT CAST(JSON_UNQUOTE(JSON_EXTRACT(Payload, '$.payload.link')) AS CHAR) FROM Outbox WHERE Topic = 'notify.requested' AND JSON_EXTRACT(Payload, '$.kind') = 'auth.password_reset'")
        .fetch_one(&h.db.pool)
        .await
        .unwrap();
    let reset_token = link.split("token=").nth(1).unwrap();
    let r =
        call(&h.app, "POST", "/auth/password/reset", None, Some(json!({"token": reset_token, "password": "a whole new passphrase"})), &[])
            .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.json);
    let old = call(&h.app, "POST", "/auth/login", None, Some(json!({"login": "mail", "password": "correct horse battery"})), &[]).await;
    assert_eq!(old.status, StatusCode::UNAUTHORIZED);
    let new = call(&h.app, "POST", "/auth/login", None, Some(json!({"login": "mail", "password": "a whole new passphrase"})), &[]).await;
    assert_eq!(new.status, StatusCode::OK);
    let _ = (token, user);
    h.db.drop_db().await;
}

#[tokio::test]
async fn invites_need_verified_email_and_idempotency_replays() {
    let Some(h) = common::harness().await else { return };
    let (token, user, _) = signup(&h.app, &h.t.ctx, "inviter").await;
    let r = call(&h.app, "POST", "/invites", Some(&token), Some(json!({})), &[]).await;
    assert_eq!((r.status, r.code()), (StatusCode::FORBIDDEN, "email_unverified"));

    crux_testkit::verify_email(&h.t.ctx, user["id"].as_str().unwrap()).await;
    let key = [("idempotency-key", "01JCREATEINVITE0000000000")];
    let a = call(&h.app, "POST", "/invites", Some(&token), Some(json!({"maxUses": 2})), &key).await;
    assert_eq!(a.status, StatusCode::CREATED, "{}", a.json);
    let b = call(&h.app, "POST", "/invites", Some(&token), Some(json!({"maxUses": 2})), &key).await;
    assert_eq!(b.status, StatusCode::CREATED);
    assert_eq!(a.json["code"], b.json["code"], "same key, same response");
    assert_eq!(b.headers.get("idempotent-replay").unwrap(), "true");
    let n: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM Invite WHERE InvitedBy IS NOT NULL").fetch_one(&h.db.pool).await.unwrap();
    assert_eq!(n.0, 1, "no duplicate row");

    let code = a.json["code"].as_str().unwrap();
    let joined = call(&h.app, "POST", "/auth/signup", None, Some(signup_body(code, "invitee")), &[]).await;
    assert_eq!(joined.status, StatusCode::OK);

    let list = call(&h.app, "GET", "/invites", Some(&token), None, &[]).await;
    assert_eq!(list.json["items"][0]["uses"], 1);
    let id = list.json["items"][0]["id"].as_str().unwrap();
    let del = call(&h.app, "DELETE", &format!("/invites/{id}"), Some(&token), None, &[]).await;
    assert_eq!(del.status, StatusCode::NO_CONTENT);
    let after = call(&h.app, "POST", "/auth/signup", None, Some(signup_body(code, "late")), &[]).await;
    assert_eq!(after.json["error"]["details"]["reason"], "revoked");
    h.db.drop_db().await;
}

#[tokio::test]
async fn sse_tickets_are_single_use() {
    let Some(h) = common::harness().await else { return };
    let (token, user, _) = signup(&h.app, &h.t.ctx, "sse").await;
    let r = call(&h.app, "POST", "/sse-ticket", Some(&token), None, &[]).await;
    let ticket = r.json["ticket"].as_str().unwrap();
    let holder = crux_service::sse::redeem(&h.t.ctx, ticket).await.unwrap();
    assert_eq!(holder.user_ref, user["id"].as_str().unwrap());
    assert!(crux_service::sse::redeem(&h.t.ctx, ticket).await.is_err());
    h.db.drop_db().await;
}

#[tokio::test]
async fn admin_changes_tier_with_audit_and_event() {
    let Some(h) = common::harness().await else { return };
    let (admin_token, admin, _) = signup(&h.app, &h.t.ctx, "boss").await;
    crux_testkit::make_admin(&h.t.ctx, admin["id"].as_str().unwrap()).await;
    let (_, user, _) = signup(&h.app, &h.t.ctx, "member").await;

    let t = call(
        &h.app,
        "POST",
        "/admin/tiers",
        Some(&admin_token),
        Some(json!({"code": "plus", "name": "Plus", "storageQuotaBytes": 5_368_709_120i64})),
        &[],
    )
    .await;
    assert_eq!(t.status, StatusCode::CREATED, "{}", t.json);

    let uid = user["id"].as_str().unwrap();
    let p = call(
        &h.app,
        "PATCH",
        &format!("/admin/users/{uid}"),
        Some(&admin_token),
        Some(json!({"tierCode": "plus", "reason": "beta tester"})),
        &[],
    )
    .await;
    assert_eq!(p.status, StatusCode::OK, "{}", p.json);
    assert_eq!(p.json["tier"], "plus");

    let topics = outbox_topics(&h.db.pool).await;
    assert!(topics.contains(&"user.tier_changed".to_string()));
    let audit: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM UserEvent WHERE Type = 'admin.user_update' AND ActorUserId IS NOT NULL")
        .fetch_one(&h.db.pool)
        .await
        .unwrap();
    assert_eq!(audit.0, 1);
    let change: (String,) =
        sqlx::query_as("SELECT Reason FROM UserTierChange WHERE ChangedBy IS NOT NULL").fetch_one(&h.db.pool).await.unwrap();
    assert_eq!(change.0, "beta tester");
    h.db.drop_db().await;
}
