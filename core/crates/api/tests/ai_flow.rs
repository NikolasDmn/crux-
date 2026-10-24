//! Milestone 8 on core's side: the gate through ai, and what core tells ai (spec 7.1, 7.4, 9.2).

mod common;

use axum::http::StatusCode;
use crux_testkit::{call, signup};
use serde_json::json;

#[tokio::test]
async fn gate_materials_and_context() {
    let Some(h) = common::harness().await else { return };
    let (t, me, _) = signup(&h.app, &h.t.ctx, "ada").await;
    let me = me["id"].as_str().unwrap().to_string();
    let (_, bob, _) = signup(&h.app, &h.t.ctx, "bob").await;
    let bob = bob["id"].as_str().unwrap().to_string();
    let course = call(&h.app, "POST", "/courses", Some(&t), Some(json!({"name": "Stats", "code": "ST"})), &[]).await.json["id"]
        .as_str()
        .unwrap()
        .to_string();
    let l1 = call(&h.app, "POST", &format!("/courses/{course}/components"), Some(&t), Some(json!({"code": "L1", "name": "Intro"})), &[])
        .await
        .json["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mats = format!("/components/{l1}/materials");
    let m = call(&h.app, "POST", &mats, Some(&t), Some(json!({"kind": "Notes", "title": "Notes", "content": "Var(aX) = a^2 Var(X)"})), &[])
        .await
        .json;
    let hidden =
        call(&h.app, "POST", &mats, Some(&t), Some(json!({"kind": "Notes", "title": "Licensed", "content": "x", "aiAllowed": false})), &[])
            .await
            .json;
    let q = call(&h.app, "POST", &format!("/courses/{course}/questions"), Some(&t), Some(json!({
        "componentId": l1, "type": "MC",
        "content": {"prompt": "Var(2X)?", "explanation": "Square it.", "options": [{"text": "4Var(X)", "isCorrect": true}, {"text": "2Var(X)", "isCorrect": false}]}
    })), &[]).await.json;
    call(&h.app, "POST", "/slips", Some(&t), Some(json!({"courseId": course, "type": "Concept", "description": "forgot to square"})), &[])
        .await;

    // Gate: ai unreachable -> service_unavailable; then what ai answers.
    let r = call(&h.app, "GET", "/me/ai-allowed?feature=MaterialChat", Some(&t), None, &[]).await;
    assert_eq!((r.json["allowed"].as_bool(), r.json["reason"].as_str()), (Some(false), Some("service_unavailable")));
    h.t.ai.set(&me, "MaterialChat", None);
    let r = call(&h.app, "GET", &format!("/me/ai-allowed?feature=MaterialChat&courseId={course}"), Some(&t), None, &[]).await;
    assert_eq!(r.json["allowed"], true);
    h.t.ai.set(&me, "Grading", Some("no_key"));
    let r = call(&h.app, "POST", &format!("/courses/{course}/mocks"), Some(&t), Some(json!({"gradingMode": "AI"})), &[]).await;
    assert_eq!((r.status, r.json["error"]["details"]["reason"].as_str()), (StatusCode::FORBIDDEN, Some("no_key")));

    // Internal: only with a service token; only what the user can read.
    let svc = h.t.ctx.keys.sign_service("ai", "core", h.t.ctx.clock.now().timestamp());
    let auth = format!("Bearer {svc}");
    let headers = [("authorization", auth.as_str())];
    let refs = format!("{},{}", m["id"].as_str().unwrap(), hidden["id"].as_str().unwrap());
    let r = call(&h.app, "GET", &format!("/internal/v1/ai/materials?user={me}&refs={refs}"), None, None, &headers).await;
    assert_eq!(r.json.as_array().map(|a| a.len()), Some(2), "{}", r.json);
    assert_eq!(r.json[1]["aiAllowed"], false);
    let r = call(&h.app, "GET", &format!("/internal/v1/ai/materials?user={bob}&refs={refs}"), None, None, &headers).await;
    assert_eq!(r.json.as_array().map(|a| a.len()), Some(0), "not bob's course");
    let r = call(&h.app, "GET", &format!("/internal/v1/ai/materials?user={me}&refs={refs}"), Some(&t), None, &[]).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED, "an access token isn't a service token");

    let qid = q["id"].as_str().unwrap();
    let body = json!({"user": me, "course": course, "scopes": ["mistakes", "answers"], "anchor": {"questionId": qid}});
    let r = call(&h.app, "POST", "/internal/v1/context/user", None, Some(body.clone()), &headers).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    let c = &r.json;
    assert_eq!(c["course"]["code"], "ST");
    assert_eq!(c["materials"].as_array().unwrap().len(), 1, "only AI-allowed materials");
    assert_eq!(c["mistakes"]["slips"][0]["description"], "forgot to square");
    assert!(c.get("progress").is_none(), "not shared");
    assert_eq!(c["answers"]["keyRevealed"], false);
    assert!(c["answers"]["question"]["options"][0].get("isCorrect").is_none() && c["answers"]["question"].get("explanation").is_none());
    let correct = q["current"]["options"].as_array().unwrap().iter().find(|o| o["isCorrect"] == true).unwrap()["id"].clone();
    call(
        &h.app,
        "POST",
        "/attempts",
        Some(&t),
        Some(json!({"clientId": "01ATTEMPT00000000000000001", "questionId": qid, "context": "Practice", "choiceOptionId": correct})),
        &[],
    )
    .await;
    let r = call(&h.app, "POST", "/internal/v1/context/user", None, Some(body), &headers).await;
    assert_eq!(
        (r.json["answers"]["keyRevealed"].as_bool(), r.json["answers"]["question"]["options"][0]["isCorrect"].as_bool()),
        (Some(true), Some(true))
    );
    let r = call(&h.app, "POST", "/internal/v1/context/user", None, Some(json!({"user": bob, "course": course})), &headers).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    h.db.drop_db().await;
}

#[tokio::test]
async fn ai_grading_decisions() {
    use crux_clients::ai::GradeSuggestion;
    let Some(h) = common::harness().await else { return };
    let (t, me, _) = signup(&h.app, &h.t.ctx, "gus").await;
    let me = me["id"].as_str().unwrap().to_string();
    let course = call(&h.app, "POST", "/courses", Some(&t), Some(json!({"name": "Stats", "code": "ST"})), &[]).await.json["id"]
        .as_str()
        .unwrap()
        .to_string();
    let l1 = call(&h.app, "POST", &format!("/courses/{course}/components"), Some(&t), Some(json!({"code": "L1", "name": "Intro"})), &[])
        .await
        .json["id"]
        .as_str()
        .unwrap()
        .to_string();
    for p in ["Explain residuals.", "Define variance."] {
        let r = call(
            &h.app,
            "POST",
            &format!("/courses/{course}/questions"),
            Some(&t),
            Some(json!({
                "componentId": l1, "type": "Open", "content": {"prompt": p, "open": {"modelAnswer": "Observed minus fitted."}}
            })),
            &[],
        )
        .await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    }
    h.t.ai.set(&me, "Grading", None);
    let mock = call(&h.app, "POST", &format!("/courses/{course}/mocks"), Some(&t), Some(json!({"gradingMode": "AI"})), &[]).await;
    assert_eq!(mock.status, StatusCode::CREATED, "{}", mock.json);
    let mid = mock.json["id"].as_str().unwrap().to_string();
    call(&h.app, "POST", &format!("/mocks/{mid}/start"), Some(&t), None, &[]).await;
    for o in [1, 2] {
        call(
            &h.app,
            "PUT",
            &format!("/mocks/{mid}/questions/{o}/answer"),
            Some(&t),
            Some(json!({"text": "Observed minus predicted"})),
            &[],
        )
        .await;
    }
    let sub = call(&h.app, "POST", &format!("/mocks/{mid}/submit"), Some(&t), None, &[]).await;
    assert_eq!(sub.json["status"], "Grading", "{}", sub.json);
    let topics = crux_testkit::outbox_topics(&h.db.pool).await;
    assert!(topics.contains(&"mock.submitted".to_string()));

    let g = |id: &str, order: i32| GradeSuggestion {
        id: id.into(),
        order,
        outcome: "Partial".into(),
        grade: 0.5,
        max_grade: 1.0,
        status: "Pending".into(),
    };
    h.t.ai.set_grades(&mid, vec![g("01AIGRADE00000000000000001", 1), g("01AIGRADE00000000000000002", 2)]);
    let wrong = call(
        &h.app,
        "POST",
        &format!("/mocks/{mid}/questions/2/ai-grade/accept"),
        Some(&t),
        Some(json!({"gradeId": "01AIGRADE00000000000000001"})),
        &[],
    )
    .await;
    assert_eq!(wrong.status, StatusCode::NOT_FOUND, "a suggestion for another item");
    let r = call(
        &h.app,
        "POST",
        &format!("/mocks/{mid}/questions/1/ai-grade/accept"),
        Some(&t),
        Some(json!({"gradeId": "01AIGRADE00000000000000001"})),
        &[],
    )
    .await;
    assert_eq!((r.status, r.json["items"][0]["gradedBy"].as_str()), (StatusCode::OK, Some("AI")), "{}", r.json);
    let r = call(
        &h.app,
        "POST",
        &format!("/mocks/{mid}/questions/2/ai-grade/override"),
        Some(&t),
        Some(json!({"gradeId": "01AIGRADE00000000000000002"})),
        &[],
    )
    .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY, "override needs an outcome");
    let r = call(
        &h.app,
        "POST",
        &format!("/mocks/{mid}/questions/2/ai-grade/override"),
        Some(&t),
        Some(json!({"gradeId": "01AIGRADE00000000000000002", "outcome": "Right"})),
        &[],
    )
    .await;
    assert_eq!((r.json["status"].as_str(), r.json["items"][1]["gradedBy"].as_str()), (Some("Graded"), Some("Self")), "{}", r.json);
    let decided = crux_testkit::outbox_topics(&h.db.pool).await.iter().filter(|t| *t == "mock.ai_grade_decided").count();
    assert_eq!(decided, 2);

    // ai asks core whether the user may open the mock.
    let svc = h.t.ctx.keys.sign_service("ai", "core", h.t.ctx.clock.now().timestamp());
    let auth = format!("Bearer {svc}");
    let r = call(&h.app, "GET", &format!("/internal/v1/mocks/{mid}/open?user={me}"), None, None, &[("authorization", auth.as_str())]).await;
    assert_eq!(r.json["status"], "Graded");
    h.db.drop_db().await;
}
