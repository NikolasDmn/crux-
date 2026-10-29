//! Cross-user harness (spec 13): generated from the route table. For every public route:
//! - without a token, every signed-in route answers 401;
//! - a non-admin gets 404 on every admin route (never 403: don't confirm it exists);
//! - a stranger gets 404 on every route that names someone else's resource;
//! - on course routes, a Viewer gets 404 on Edit routes and an Editor 404 on Manage routes.
//!
//! New routes with a path id need a fixture in `fixture_path`; the test fails until they have one.

mod common;

use axum::http::StatusCode;
use crux_api::policy_table::{operations, policy_of, Policy};
use crux_testkit::{call, signup, verify_email};
use serde_json::{json, Value};

/// Alice's resources for one route: a fresh course with a lecture, a material and a paper.
struct Fixture {
    course: String,
    component: String,
    material: String,
    paper: String,
    invite: String,
    question: String,
    mock: String,
    slip: String,
    weak_spot: String,
    trap: String,
    card: String,
    deadline: String,
    entry: String,
}

async fn fixture(h: &common::Harness, alice: &str, n: usize) -> Fixture {
    let c = call(&h.app, "POST", "/courses", Some(alice), Some(json!({"name": "C", "code": format!("C{n}")})), &[]).await.json;
    let course = c["id"].as_str().unwrap().to_string();
    let k = call(&h.app, "POST", &format!("/courses/{course}/components"), Some(alice), Some(json!({"code": "L1", "name": "L1"})), &[])
        .await
        .json;
    let component = k["id"].as_str().unwrap().to_string();
    let m = call(
        &h.app,
        "POST",
        &format!("/components/{component}/materials"),
        Some(alice),
        Some(json!({"kind": "Notes", "title": "T", "content": "x"})),
        &[],
    )
    .await
    .json;
    let p = call(&h.app, "POST", &format!("/courses/{course}/papers"), Some(alice), Some(json!({"paperKey": "P", "label": "P"})), &[])
        .await
        .json;
    // A fixture per route: expire the earlier fixtures' invites, or Alice reaches the cap on open
    // invites (audit M5).
    sqlx::query("UPDATE Invite SET RevokedAt = UTC_TIMESTAMP()").execute(&h.db.pool).await.unwrap();
    let i = call(&h.app, "POST", "/invites", Some(alice), Some(json!({})), &[]).await.json;
    let q = call(
        &h.app,
        "POST",
        &format!("/courses/{course}/questions"),
        Some(alice),
        Some(json!({
            "componentId": component, "type": "MC",
            "content": {"prompt": "Q", "options": [{"text": "a", "isCorrect": true}, {"text": "b", "isCorrect": false}]}
        })),
        &[],
    )
    .await
    .json;
    let mock = call(&h.app, "POST", &format!("/courses/{course}/mocks"), Some(alice), Some(json!({})), &[]).await.json;
    let slip = call(&h.app, "POST", "/slips", Some(alice), Some(json!({"courseId": course, "type": "Sign", "description": "minus"})), &[])
        .await
        .json;
    let spot = call(&h.app, "POST", &format!("/courses/{course}/weak-spots"), Some(alice), Some(json!({"topic": "T"})), &[]).await.json;
    let trap = call(&h.app, "POST", &format!("/courses/{course}/traps"), Some(alice), Some(json!({"text": "careful"})), &[]).await.json;
    let card = call(
        &h.app,
        "POST",
        &format!("/courses/{course}/flashcards"),
        Some(alice),
        Some(json!({"componentId": component, "front": "F", "back": "B", "shared": true})),
        &[],
    )
    .await
    .json;
    let deadline = call(
        &h.app,
        "POST",
        &format!("/courses/{course}/deadlines"),
        Some(alice),
        Some(json!({"type": "Exam", "startsAt": "2026-12-01T09:00:00Z"})),
        &[],
    )
    .await
    .json;
    let entry = call(
        &h.app,
        "POST",
        "/plan/entries",
        Some(alice),
        Some(json!({"courseId": course, "description": "Read L1", "startAt": "2026-11-02T09:00:00Z", "endAt": "2026-11-02T10:00:00Z"})),
        &[],
    )
    .await
    .json;
    Fixture {
        deadline: deadline["id"].as_str().expect("deadline fixture").to_string(),
        entry: entry["id"].as_str().expect("plan entry fixture").to_string(),
        question: q["id"].as_str().expect("question fixture").to_string(),
        mock: mock["id"].as_str().expect("mock fixture").to_string(),
        slip: slip["id"].as_str().expect("slip fixture").to_string(),
        weak_spot: spot["id"].as_str().expect("weak spot fixture").to_string(),
        trap: trap["id"].as_str().expect("trap fixture").to_string(),
        card: card["id"].as_str().expect("card fixture").to_string(),
        course,
        component,
        material: m["id"].as_str().unwrap().to_string(),
        paper: p["id"].as_str().unwrap().to_string(),
        invite: i["id"].as_str().unwrap().to_string(),
    }
}

/// The concrete path naming Alice's resource, or None if the route has no fixture yet.
fn fixture_path(path: &str, f: &Fixture, member_id: &str) -> Option<String> {
    let p = match path {
        "/invites/{id}" => path.replace("{id}", &f.invite),
        p if p.starts_with("/courses/{id}/members/{memberId}") => p.replace("{id}", &f.course).replace("{memberId}", member_id),
        p if p.starts_with("/courses/{id}/traps/{trapId}") => p.replace("{id}", &f.course).replace("{trapId}", &f.trap),
        p if p.starts_with("/questions/{id}") => p.replace("{id}", &f.question),
        p if p.starts_with("/mocks/{id}") => p.replace("{id}", &f.mock).replace("{order}", "1"),
        p if p.starts_with("/slips/{id}") => p.replace("{id}", &f.slip),
        p if p.starts_with("/weak-spots/{id}") => p.replace("{id}", &f.weak_spot),
        p if p.starts_with("/flashcards/{id}") => p.replace("{id}", &f.card),
        p if p.starts_with("/deadlines/{id}") => p.replace("{id}", &f.deadline),
        p if p.starts_with("/plan/entries/{id}") => p.replace("{id}", &f.entry),
        p if p.starts_with("/courses/{id}") => p.replace("{id}", &f.course),
        p if p.starts_with("/course-invitations/{courseId}") => p.replace("{courseId}", &f.course),
        p if p.starts_with("/components/{id}") => p.replace("{id}", &f.component),
        p if p.starts_with("/materials/{id}") => p.replace("{id}", &f.material),
        p if p.starts_with("/papers/{id}") => p.replace("{id}", &f.paper),
        _ => return None,
    };
    Some(p)
}

/// A body that deserializes, so the access check is what answers (a malformed body is 422 for
/// everyone, whether or not the resource exists, so it reveals nothing).
fn body_for(method: &str) -> Option<Value> {
    matches!(method, "POST" | "PATCH" | "PUT").then(|| json!({}))
}

fn body_for_path(method: &str, path: &str) -> Option<Value> {
    match (method, path) {
        ("POST", "/courses/{id}/concepts") => Some(json!({"name": "Probability"})),
        ("POST", "/courses/{id}/members") => Some(json!({"usernameOrEmail": "nobody-here", "role": "Viewer"})),
        ("POST", "/courses/{id}/questions") => {
            Some(json!({"componentId": "01AAAAAAAAAAAAAAAAAAAAAAAA", "type": "Open", "content": {"prompt": "x"}}))
        }
        ("POST", "/courses/{id}/weak-spots") => Some(json!({"topic": "x"})),
        ("POST", "/courses/{id}/traps") => Some(json!({"text": "x"})),
        ("POST", "/courses/{id}/flashcards") => Some(json!({"componentId": "01AAAAAAAAAAAAAAAAAAAAAAAA", "front": "x", "back": "y"})),
        ("POST", "/flashcards/{id}/warmup-flag") => Some(json!({"flagged": true})),
        ("POST", "/flashcards/{id}/suspend") => Some(json!({"suspended": true})),
        ("POST", "/courses/{id}/deadlines") => Some(json!({"startsAt": "2026-12-01T09:00:00Z"})),
        ("PUT", "/deadlines/{id}/layout") => Some(json!({"components": []})),
        ("PUT", "/materials/{id}/checks") => Some(json!({"checks": []})),
        ("PATCH", "/courses/{id}/progress") => Some(json!({"componentId": "01AAAAAAAAAAAAAAAAAAAAAAAA"})),
        ("POST", "/mocks/{id}/questions/{order}/grade") => Some(json!({"outcome": "Right"})),
        ("POST", "/mocks/{id}/questions/{order}/ai-grade/accept")
        | ("POST", "/mocks/{id}/questions/{order}/ai-grade/reject")
        | ("POST", "/mocks/{id}/questions/{order}/ai-grade/override") => Some(json!({"gradeId": "01AAAAAAAAAAAAAAAAAAAAAAAA"})),
        ("POST", "/questions/{id}/image-crop") => {
            Some(json!({"fileId": "01AAAAAAAAAAAAAAAAAAAAAAAA", "page": 1, "box": {"x": 0, "y": 0, "w": 1, "h": 1}}))
        }
        _ => body_for(method),
    }
}

/// PATCH routes need If-Match; send one so the access check is what's tested.
fn headers_for(method: &str) -> &'static [(&'static str, &'static str)] {
    if method == "PATCH" {
        &[("if-match", "0")]
    } else {
        &[]
    }
}

/// Shares the course with `who` as `role` and accepts.
async fn share(h: &common::Harness, alice: &str, course: &str, who: (&str, &str), role: &str) {
    let r = call(
        &h.app,
        "POST",
        &format!("/courses/{course}/members"),
        Some(alice),
        Some(json!({"usernameOrEmail": who.0, "role": role})),
        &[],
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    let a = call(&h.app, "POST", &format!("/course-invitations/{course}/accept"), Some(who.1), None, &[]).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.json);
}

#[tokio::test]
async fn cross_user_access_is_404() {
    let Some(h) = common::harness().await else { return };
    let api = crux_api::openapi_doc();
    let ops = operations(&api);

    let (alice, a, _) = signup(&h.app, &h.t.ctx, "alice").await;
    verify_email(&h.t.ctx, a["id"].as_str().unwrap()).await;
    let (bob, b, _) = signup(&h.app, &h.t.ctx, "bob").await;
    verify_email(&h.t.ctx, b["id"].as_str().unwrap()).await;
    let (viewer, v, _) = signup(&h.app, &h.t.ctx, "vera").await;
    let vera_id = v["id"].as_str().unwrap().to_string();
    let (editor, _, _) = signup(&h.app, &h.t.ctx, "eddie").await;

    for (n, (method, path)) in ops.iter().enumerate() {
        let policy = policy_of(method, path).unwrap();
        let placeholder = path
            .replace("{order}", "1")
            .replace("{trapId}", "01AAAAAAAAAAAAAAAAAAAAAAAA")
            .replace("{id}", "01AAAAAAAAAAAAAAAAAAAAAAAA")
            .replace("{code}", "free")
            .replace("{memberId}", "01AAAAAAAAAAAAAAAAAAAAAAAA")
            .replace("{courseId}", "01AAAAAAAAAAAAAAAAAAAAAAAA");

        if policy != Policy::Public {
            let anon = call(&h.app, method, &placeholder, None, body_for(method), headers_for(method)).await;
            assert_eq!(anon.status, StatusCode::UNAUTHORIZED, "{method} {path} without a token");
        }
        if policy == Policy::Admin {
            let r = call(&h.app, method, &placeholder, Some(&bob), body_for(method), headers_for(method)).await;
            assert_eq!(r.status, StatusCode::NOT_FOUND, "{method} {path} as a non-admin must be 404");
            continue;
        }
        if !path.contains('{') || policy == Policy::Public {
            continue;
        }

        let f = fixture(&h, &alice, n).await;
        let member_route = path.ends_with("{memberId}");
        if member_route {
            // The member to remove: vera. (She may remove herself: leaving is allowed.)
            share(&h, &alice, &f.course, ("vera", &viewer), "Viewer").await;
        }
        let restore_route = path.ends_with("/restore");
        if restore_route {
            // Restore acts on a trashed course: share while it's live, then trash it.
            share(&h, &alice, &f.course, ("vera", &viewer), "Viewer").await;
            share(&h, &alice, &f.course, ("eddie", &editor), "Editor").await;
            call(&h.app, "DELETE", &format!("/courses/{}", f.course), Some(&alice), None, &[]).await;
        }
        let Some(target) = fixture_path(path, &f, &vera_id) else {
            panic!("{method} {path} has a path id but no cross-user fixture in fixture_path");
        };
        let r = call(&h.app, method, &target, Some(&bob), body_for_path(method, path), headers_for(method)).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{method} {target}: a stranger must not reach alice's resource");

        // Role boundaries inside a shared course.
        if matches!(policy, Policy::Edit | Policy::Manage) && !member_route {
            if !restore_route {
                share(&h, &alice, &f.course, ("vera", &viewer), "Viewer").await;
            }
            let r = call(&h.app, method, &target, Some(&viewer), body_for_path(method, path), headers_for(method)).await;
            assert_eq!(r.status, StatusCode::NOT_FOUND, "{method} {path}: a Viewer can't use an {policy:?} route");
        }
        if policy == Policy::Manage {
            if !restore_route {
                share(&h, &alice, &f.course, ("eddie", &editor), "Editor").await;
            }
            let r = call(&h.app, method, &target, Some(&editor), body_for_path(method, path), headers_for(method)).await;
            assert_eq!(r.status, StatusCode::NOT_FOUND, "{method} {path}: an Editor can't use a Manage route");
        }

        // Per-user resources: even a member of alice's course gets 404 (review section 2).
        if policy == Policy::Mine || policy == Policy::Subscribed {
            share(&h, &alice, &f.course, ("vera", &viewer), "Viewer").await;
            let r = call(&h.app, method, &target, Some(&viewer), body_for_path(method, path), headers_for(method)).await;
            assert_eq!(
                r.status,
                StatusCode::NOT_FOUND,
                "{method} {path}: a course member must not reach alice's own data (or need a subscription)"
            );
        }

        // And alice isn't refused her own (bodies are empty, so 422 is fine; 404 is not).
        // (Invitations have none to accept; the fixture question is already in the bank, so there's
        // nothing to promote. Both are covered in their own flow tests.)
        if !path.starts_with("/course-invitations") && !path.ends_with("/promote") {
            let r = call(&h.app, method, &target, Some(&alice), body_for_path(method, path), headers_for(method)).await;
            assert_ne!(r.status, StatusCode::NOT_FOUND, "{method} {target}: alice can reach her own");
        }
    }
    h.db.drop_db().await;
}
