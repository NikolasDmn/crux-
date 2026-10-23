//! Cross-user harness (spec 13): generated from the route table. For every public route:
//! - without a token, every signed-in route answers 401;
//! - a non-admin gets 404 on every admin route (never 403: don't confirm it exists);
//! - user B gets 404 on every route that names user A's resource.
//!
//! New routes with a path id need a fixture in `resource_of_a`; the test fails until they have one.

mod common;

use axum::http::StatusCode;
use crux_api::policy_table::{operations, policy_of, Policy};
use crux_testkit::{call, signup, verify_email};
use serde_json::json;

/// Creates user A's resource for a parameterised path and returns the concrete path.
async fn resource_of_a(h: &common::Harness, a_token: &str, method: &str, path: &str) -> Option<String> {
    match (method, path) {
        ("DELETE", "/invites/{id}") => {
            let r = call(&h.app, "POST", "/invites", Some(a_token), Some(json!({})), &[]).await;
            assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
            Some(format!("/invites/{}", r.json["id"].as_str().unwrap()))
        }
        _ => None,
    }
}

fn body_for(method: &str) -> Option<serde_json::Value> {
    matches!(method, "POST" | "PATCH" | "PUT").then(|| json!({}))
}

#[tokio::test]
async fn cross_user_access_is_404() {
    let Some(h) = common::harness().await else { return };
    let api = crux_api::openapi_doc();
    let ops = operations(&api);

    let (a_token, a, _) = signup(&h.app, &h.t.ctx, "alice").await;
    verify_email(&h.t.ctx, a["id"].as_str().unwrap()).await;
    let (b_token, b, _) = signup(&h.app, &h.t.ctx, "bob").await;
    verify_email(&h.t.ctx, b["id"].as_str().unwrap()).await;

    for (method, path) in &ops {
        let policy = policy_of(method, path).unwrap();
        let concrete = path.replace("{id}", "01AAAAAAAAAAAAAAAAAAAAAAAA").replace("{code}", "free");

        if policy != Policy::Public {
            let anon = call(&h.app, method, &concrete, None, body_for(method), &[]).await;
            assert_eq!(anon.status, StatusCode::UNAUTHORIZED, "{method} {path} without a token");
        }

        if policy == Policy::Admin {
            let r = call(&h.app, method, &concrete, Some(&b_token), body_for(method), &[]).await;
            assert_eq!(r.status, StatusCode::NOT_FOUND, "{method} {path} as a non-admin must be 404");
            continue;
        }

        if path.contains('{') && policy != Policy::Public {
            let Some(target) = resource_of_a(&h, &a_token, method, path).await else {
                panic!("{method} {path} has a path id but no cross-user fixture in resource_of_a");
            };
            let r = call(&h.app, method, &target, Some(&b_token), body_for(method), &[]).await;
            assert_eq!(r.status, StatusCode::NOT_FOUND, "{method} {target}: bob must not reach alice's resource");
            let r = call(&h.app, method, &target, Some(&a_token), body_for(method), &[]).await;
            assert!(r.status.is_success(), "{method} {target}: alice can reach her own ({})", r.status);
        }
    }
    h.db.drop_db().await;
}
