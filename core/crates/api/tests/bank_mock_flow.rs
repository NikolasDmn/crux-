//! Milestone 3: bank, mocks, grading, review, slips, weak spots, attempts (spec 5.2-5.6).

mod common;

use axum::http::StatusCode;
use crux_testkit::{call, signup};
use serde_json::{json, Value};

struct Course {
    id: String,
    l1: String,
    concept: String,
}

async fn setup(h: &common::Harness, token: &str) -> Course {
    let c = call(&h.app, "POST", "/courses", Some(token), Some(json!({"name": "Stats", "code": "ST"})), &[]).await.json;
    let id = c["id"].as_str().unwrap().to_string();
    let l1 = call(&h.app, "POST", &format!("/courses/{id}/components"), Some(token), Some(json!({"code": "L1", "name": "Basics"})), &[])
        .await
        .json;
    let concept = call(&h.app, "POST", &format!("/courses/{id}/concepts"), Some(token), Some(json!({"name": "Means"})), &[]).await.json;
    Course { id, l1: l1["id"].as_str().unwrap().into(), concept: concept["id"].as_str().unwrap().into() }
}

async fn mc(h: &common::Harness, token: &str, c: &Course, prompt: &str) -> Value {
    let r = call(&h.app, "POST", &format!("/courses/{}/questions", c.id), Some(token), Some(json!({
        "componentId": c.l1, "type": "MC", "conceptIds": [c.concept], "primaryConceptId": c.concept,
        "content": {"prompt": prompt, "explanation": "Because.", "options": [
            {"text": "Right one", "isCorrect": true}, {"text": "Wrong one", "isCorrect": false}, {"text": "Also wrong", "isCorrect": false}]}
    })), &[]).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    r.json
}

fn option(q: &Value, correct: bool) -> String {
    q["current"]["options"].as_array().unwrap().iter().find(|o| o["isCorrect"] == correct).unwrap()["id"].as_str().unwrap().into()
}

#[tokio::test]
async fn bank_versions_types_and_visibility() {
    let Some(h) = common::harness().await else { return };
    let (owner, _, _) = signup(&h.app, &h.t.ctx, "owner").await;
    let (viewer, _, _) = signup(&h.app, &h.t.ctx, "viewer").await;
    let c = setup(&h, &owner).await;
    let q = mc(&h, &owner, &c, "Which is right?").await;
    assert_eq!((q["current"]["versionNo"].as_i64(), q["private"].as_bool()), (Some(1), Some(false)));

    let bad = call(&h.app, "POST", &format!("/courses/{}/questions", c.id), Some(&owner), Some(json!({
        "componentId": c.l1, "type": "MC", "content": {"prompt": "x", "options": [{"text": "a", "isCorrect": true}, {"text": "b", "isCorrect": true}]}
    })), &[]).await;
    assert_eq!(bad.json["error"]["details"]["fields"][0]["code"], "correct", "exactly one correct");

    let numeric = call(&h.app, "POST", &format!("/courses/{}/questions", c.id), Some(&owner), Some(json!({
        "componentId": c.l1, "type": "Numeric", "content": {"prompt": "g?", "numeric": {"value": 9.81, "tolerance": 0.01, "unit": "m/s^2", "unitRequired": true}}
    })), &[]).await;
    assert_eq!(numeric.status, StatusCode::CREATED, "{}", numeric.json);

    let id = q["id"].as_str().unwrap();
    let edited = call(
        &h.app,
        "PATCH",
        &format!("/questions/{id}"),
        Some(&owner),
        Some(json!({"content": {
            "prompt": "Which is right? (v2)", "options": [{"text": "Right one", "isCorrect": true}, {"text": "Nope", "isCorrect": false}]
        }})),
        &[("if-match", &q["version"].to_string())],
    )
    .await;
    assert_eq!(edited.json["current"]["versionNo"], 2, "{}", edited.json);
    assert_eq!(
        call(&h.app, "GET", &format!("/questions/{id}/versions"), Some(&owner), None, &[]).await.json.as_array().unwrap().len(),
        2,
        "versions are kept"
    );

    // Private questions stay private; promote moves them to the bank.
    call(
        &h.app,
        "POST",
        &format!("/courses/{}/members", c.id),
        Some(&owner),
        Some(json!({"usernameOrEmail": "viewer", "role": "Viewer"})),
        &[],
    )
    .await;
    call(&h.app, "POST", &format!("/course-invitations/{}/accept", c.id), Some(&viewer), None, &[]).await;
    let private = call(
        &h.app,
        "POST",
        &format!("/courses/{}/questions", c.id),
        Some(&viewer),
        Some(json!({
            "componentId": c.l1, "type": "Open", "private": true, "content": {"prompt": "My own note-to-self question"}
        })),
        &[],
    )
    .await;
    assert_eq!((private.status, private.json["private"].as_bool()), (StatusCode::CREATED, Some(true)), "{}", private.json);
    let owner_bank = call(&h.app, "GET", &format!("/courses/{}/questions", c.id), Some(&owner), None, &[]).await;
    assert_eq!(owner_bank.json["items"].as_array().unwrap().len(), 2, "the owner doesn't see the viewer's private question");
    let pid = private.json["id"].as_str().unwrap();
    assert_eq!(call(&h.app, "GET", &format!("/questions/{pid}"), Some(&owner), None, &[]).await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        call(&h.app, "POST", &format!("/questions/{pid}/promote"), Some(&viewer), None, &[]).await.status,
        StatusCode::NOT_FOUND,
        "viewers can't add to the bank"
    );
    let viewer_bank = call(&h.app, "GET", &format!("/courses/{}/questions?type=MC", c.id), Some(&viewer), None, &[]).await;
    assert_eq!(viewer_bank.json["items"].as_array().unwrap().len(), 1, "type filter");
    h.db.drop_db().await;
}

#[tokio::test]
async fn mock_take_grade_review_and_stats() {
    let Some(h) = common::harness().await else { return };
    let (t, _, _) = signup(&h.app, &h.t.ctx, "student").await;
    let c = setup(&h, &t).await;
    let q1 = mc(&h, &t, &c, "Q1").await;
    let q2 = mc(&h, &t, &c, "Q2").await;
    call(&h.app, "POST", &format!("/courses/{}/questions", c.id), Some(&t), Some(json!({
        "componentId": c.l1, "type": "Numeric", "content": {"prompt": "angle?", "numeric": {"value": 90, "tolerance": 0, "toleranceKind": "Absolute", "unit": "°", "unitRequired": true}}
    })), &[]).await;
    call(
        &h.app,
        "POST",
        &format!("/courses/{}/questions", c.id),
        Some(&t),
        Some(json!({
            "componentId": c.l1, "type": "Open", "content": {"prompt": "Explain the mean.", "open": {"modelAnswer": "Sum over n."}}
        })),
        &[],
    )
    .await;
    call(&h.app, "POST", &format!("/courses/{}/traps", c.id), Some(&t), Some(json!({"text": "Read the units twice"})), &[]).await;

    let m = call(
        &h.app,
        "POST",
        &format!("/courses/{}/mocks", c.id),
        Some(&t),
        Some(json!({"n": 10, "timeLimitMin": 30, "goEasy": false})),
        &[],
    )
    .await;
    assert_eq!(m.status, StatusCode::CREATED, "{}", m.json);
    assert_eq!(m.json["items"].as_array().unwrap().len(), 4);
    assert_eq!(m.json["warnings"][0]["code"], "pool_short", "asked for 10, the pool has 4");
    assert!(m.json["items"][0]["options"].is_null() || m.json["items"].as_array().unwrap().iter().all(|i| i["key"].is_null()));
    let mid = m.json["id"].as_str().unwrap().to_string();
    let item_of =
        |qid: &str| m.json["items"].as_array().unwrap().iter().find(|i| i["questionId"] == qid).unwrap()["order"].as_i64().unwrap();
    let typed = |ty: &str| m.json["items"].as_array().unwrap().iter().find(|i| i["type"] == ty).unwrap()["order"].as_i64().unwrap();

    let early = call(&h.app, "PUT", &format!("/mocks/{mid}/questions/1/answer"), Some(&t), Some(json!({})), &[]).await;
    assert_eq!(early.status, StatusCode::CONFLICT, "answering before start");
    call(&h.app, "POST", &format!("/mocks/{mid}/start"), Some(&t), None, &[]).await;
    let ans = |order: i64, body: Value| (format!("/mocks/{mid}/questions/{order}/answer"), body);
    for (path, body) in [
        ans(item_of(q1["id"].as_str().unwrap()), json!({"choiceOptionId": option(&q1, true), "confidence": "Sure"})),
        ans(item_of(q2["id"].as_str().unwrap()), json!({"choiceOptionId": option(&q2, false), "confidence": "Sure"})),
        ans(typed("Numeric"), json!({"numeric": 90, "unit": "deg", "confidence": "Unsure"})),
        ans(typed("Open"), json!({"text": "Add them up and divide by how many.", "confidence": "Unsure"})),
    ] {
        let r = call(&h.app, "PUT", &path, Some(&t), Some(body), &[]).await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.json);
        assert!(r.json["key"].is_null() && r.json["outcome"].is_null(), "no key or outcome while taking");
    }

    let sub = call(&h.app, "POST", &format!("/mocks/{mid}/submit"), Some(&t), None, &[]).await;
    assert_eq!(sub.json["status"], "Submitted", "the open answer waits for a self-grade: {}", sub.json);
    let items = sub.json["items"].as_array().unwrap();
    let by_order = |o: i64| items.iter().find(|i| i["order"] == o).unwrap().clone();
    assert_eq!(by_order(item_of(q1["id"].as_str().unwrap()))["outcome"], "Right");
    let wrong = by_order(item_of(q2["id"].as_str().unwrap()));
    assert_eq!(wrong["outcome"], "Wrong");
    assert_eq!(by_order(typed("Numeric"))["outcome"], "Right", "deg = ° via the alias table");

    let graded = call(
        &h.app,
        "POST",
        &format!("/mocks/{mid}/questions/{}/grade", typed("Open")),
        Some(&t),
        Some(json!({"outcome": "Partial"})),
        &[],
    )
    .await;
    assert_eq!(graded.json["status"], "Graded", "{}", graded.json);
    assert_eq!((graded.json["score"].as_f64(), graded.json["maxScore"].as_f64()), (Some(2.5), Some(4.0)), "Partial counts 0.5");

    let result = call(&h.app, "GET", &format!("/mocks/{mid}/result"), Some(&t), None, &[]).await.json;
    let w = result["items"].as_array().unwrap().iter().find(|i| i["outcome"] == "Wrong").unwrap().clone();
    assert!(w["key"].is_null(), "a miss keeps its key hidden");
    assert!(w["options"].as_array().unwrap().iter().all(|o| o["isCorrect"].is_null()));
    assert_eq!(w["slipProposed"], true);
    let r = result["items"].as_array().unwrap().iter().find(|i| i["outcome"] == "Right").unwrap();
    assert!(r["key"].is_object(), "right items show their key");

    // Confirm the proposed slip; it disappears from the proposals.
    let order = w["order"].as_i64().unwrap();
    let slip = call(
        &h.app,
        "POST",
        "/slips",
        Some(&t),
        Some(
            json!({"courseId": c.id, "type": "Misread", "description": "Picked the distractor", "mockId": mid, "mockQuestionOrder": order}),
        ),
        &[],
    )
    .await;
    assert_eq!(slip.status, StatusCode::CREATED, "{}", slip.json);

    let shown = call(&h.app, "POST", &format!("/mocks/{mid}/questions/{order}/show-answer"), Some(&t), None, &[]).await.json;
    let w2 = shown["items"].as_array().unwrap().iter().find(|i| i["order"] == order).unwrap();
    assert!(w2["key"].is_object() && w2["slipProposed"] == false);
    assert_eq!(shown["status"], "Graded", "the partial still needs fixing");
    let done = call(&h.app, "POST", &format!("/mocks/{mid}/questions/{}/fixed", typed("Open")), Some(&t), None, &[]).await.json;
    assert_eq!((done["status"].as_str(), done["revealAll"].as_bool()), (Some("Reviewed"), Some(true)));

    // A later right answer resolves the slip (spec 5.6); attempts are idempotent on clientId.
    let att = json!({"clientId": "01CLIENT000000000000000001", "questionId": q2["id"], "context": "Practice", "choiceOptionId": option(&q2, true), "confidence": "Sure"});
    let a1 = call(&h.app, "POST", "/attempts", Some(&t), Some(att.clone()), &[]).await;
    assert_eq!((a1.json["outcome"].as_str(), a1.json["duplicate"].as_bool()), (Some("Right"), Some(false)), "{}", a1.json);
    let a2 = call(&h.app, "POST", "/attempts", Some(&t), Some(att), &[]).await;
    assert_eq!(a2.json["duplicate"], true);
    let slips = call(&h.app, "GET", "/slips?status=open", Some(&t), None, &[]).await;
    assert_eq!(slips.json.as_array().unwrap().len(), 0, "resolved by the right attempt");

    let stats: (i32, i32) =
        sqlx::query_as("SELECT Attempts, Rights FROM UserQuestionStat s JOIN Question q ON q.Id = s.QuestionId WHERE q.PublicId = ?")
            .bind(q2["id"].as_str().unwrap())
            .fetch_one(&h.db.pool)
            .await
            .unwrap();
    assert_eq!(stats, (2, 1), "one wrong in the mock, one right attempt (the duplicate didn't count)");

    let conf = call(&h.app, "GET", &format!("/courses/{}/stats/confidence", c.id), Some(&t), None, &[]).await.json;
    assert_eq!(conf[0]["sure"]["answered"], 3);
    let traps = call(&h.app, "GET", &format!("/courses/{}/traps", c.id), Some(&t), None, &[]).await.json;
    assert_eq!(traps[0]["text"], "Read the units twice");
    h.db.drop_db().await;
}

#[tokio::test]
async fn time_limit_and_misses_only() {
    let Some(h) = common::harness().await else { return };
    let (t, _, _) = signup(&h.app, &h.t.ctx, "timed").await;
    let c = setup(&h, &t).await;
    let q1 = mc(&h, &t, &c, "Q1").await;
    let q2 = mc(&h, &t, &c, "Q2").await;
    let m = call(&h.app, "POST", &format!("/courses/{}/mocks", c.id), Some(&t), Some(json!({"timeLimitMin": 10})), &[]).await.json;
    let mid = m["id"].as_str().unwrap();
    call(&h.app, "POST", &format!("/mocks/{mid}/start"), Some(&t), None, &[]).await;
    let q1_order = m["items"].as_array().unwrap().iter().find(|i| i["questionId"] == q1["id"]).unwrap()["order"].as_i64().unwrap();
    call(
        &h.app,
        "PUT",
        &format!("/mocks/{mid}/questions/{q1_order}/answer"),
        Some(&t),
        Some(json!({"choiceOptionId": option(&q1, false)})),
        &[],
    )
    .await;

    h.t.clock.advance(chrono::Duration::minutes(11));
    let grace = call(
        &h.app,
        "PUT",
        &format!("/mocks/{mid}/questions/{q1_order}/answer"),
        Some(&t),
        Some(json!({"choiceOptionId": option(&q1, false)})),
        &[],
    )
    .await;
    assert_eq!(grace.status, StatusCode::OK, "within the 2-minute grace");
    h.t.clock.advance(chrono::Duration::minutes(2));
    let late = call(&h.app, "PUT", &format!("/mocks/{mid}/questions/{q1_order}/answer"), Some(&t), Some(json!({})), &[]).await;
    assert_eq!((late.status, late.json["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("mock_time_over")));

    // The access token outlived the clock jump? Log in again to be safe.
    let t = call(&h.app, "POST", "/auth/login", None, Some(json!({"login": "timed", "password": "correct horse battery"})), &[]).await.json
        ["accessToken"]
        .as_str()
        .unwrap()
        .to_string();
    let sub = call(&h.app, "POST", &format!("/mocks/{mid}/submit"), Some(&t), None, &[]).await;
    assert_eq!(sub.json["status"], "Graded", "{}", sub.json);

    let misses = call(&h.app, "POST", &format!("/courses/{}/mocks", c.id), Some(&t), Some(json!({"missesOnly": true})), &[]).await.json;
    let ids: Vec<&str> = misses["items"].as_array().unwrap().iter().map(|i| i["questionId"].as_str().unwrap()).collect();
    assert!(ids.contains(&q1["id"].as_str().unwrap()) && ids.contains(&q2["id"].as_str().unwrap()), "wrong and blank both count as misses");
    h.db.drop_db().await;
}

#[tokio::test]
async fn weak_spots_check_resolve_and_reopen() {
    let Some(h) = common::harness().await else { return };
    let (t, _, _) = signup(&h.app, &h.t.ctx, "learner").await;
    let c = setup(&h, &t).await;
    let q = mc(&h, &t, &c, "Means?").await;
    let spot = call(&h.app, "POST", &format!("/courses/{}/weak-spots", c.id), Some(&t), Some(json!({
        "topic": "Mean vs median", "conceptId": c.concept, "severity": "Low", "coldPassesNeeded": 2, "keyRule": "\\bar{x} = \\frac{1}{n}\\sum x_i"
    })), &[]).await;
    assert_eq!(spot.status, StatusCode::CREATED, "{}", spot.json);

    let attempt = |n: u32, right: bool, hours: i64| {
        let body = json!({"clientId": format!("01CLIENT{:018}", n), "questionId": q["id"], "context": "Warmup",
            "choiceOptionId": option(&q, right), "confidence": "Sure"});
        (body, hours)
    };
    let login = || async {
        call(&h.app, "POST", "/auth/login", None, Some(json!({"login": "learner", "password": "correct horse battery"})), &[]).await.json
            ["accessToken"]
            .as_str()
            .unwrap()
            .to_string()
    };
    for (body, hours) in [attempt(1, false, 0), attempt(2, false, 1), attempt(3, false, 2)] {
        h.t.clock.advance(chrono::Duration::hours(hours));
        let r = call(&h.app, "POST", "/attempts", Some(&login().await), Some(body), &[]).await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    }
    let t = login().await;
    let view = call(&h.app, "GET", &format!("/courses/{}/weak-spots", c.id), Some(&t), None, &[]).await.json;
    assert_eq!(view["open"][0]["severity"], "Medium", "3 misses at Low → Medium: {view}");

    for n in 4..=5 {
        h.t.clock.advance(chrono::Duration::hours(30));
        let fresh = call(&h.app, "POST", "/auth/login", None, Some(json!({"login": "learner", "password": "correct horse battery"})), &[])
            .await
            .json["accessToken"]
            .as_str()
            .unwrap()
            .to_string();
        let r = call(&h.app, "POST", "/attempts", Some(&fresh), Some(attempt(n, true, 0).0), &[]).await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    }
    let fresh = call(&h.app, "POST", "/auth/login", None, Some(json!({"login": "learner", "password": "correct horse battery"})), &[])
        .await
        .json["accessToken"]
        .as_str()
        .unwrap()
        .to_string();
    let view = call(&h.app, "GET", &format!("/courses/{}/weak-spots", c.id), Some(&fresh), None, &[]).await.json;
    assert_eq!(view["resolved"][0]["topic"], "Mean vs median", "two cold passes resolve it: {view}");
    call(&h.app, "POST", "/attempts", Some(&fresh), Some(attempt(6, false, 0).0), &[]).await;
    let view = call(&h.app, "GET", &format!("/courses/{}/weak-spots", c.id), Some(&fresh), None, &[]).await.json;
    assert_eq!(view["open"][0]["status"], "Reopened");
    h.db.drop_db().await;
}

#[tokio::test]
async fn studying_the_lecture_makes_a_practice_pass_warm() {
    let Some(h) = common::harness().await else { return };
    let (t, _, _) = signup(&h.app, &h.t.ctx, "reader").await;
    let c = setup(&h, &t).await;
    let q = mc(&h, &t, &c, "Median?").await;
    let spot = call(
        &h.app,
        "POST",
        &format!("/courses/{}/weak-spots", c.id),
        Some(&t),
        Some(json!({
            "topic": "Median", "conceptId": c.concept, "severity": "Low", "coldPassesNeeded": 1
        })),
        &[],
    )
    .await;
    assert_eq!(spot.status, StatusCode::CREATED, "{}", spot.json);
    let practice = |n: u32| json!({"clientId": format!("01COLDCK{:018}", n), "questionId": q["id"], "context": "Practice", "choiceOptionId": option(&q, true), "confidence": "Sure"});

    // Reading the lecture 10 minutes ago: the right answer is warm and doesn't count.
    let ago = crux_service::time::iso(h.t.ctx.clock.now() - chrono::Duration::minutes(10));
    let ev = call(&h.app, "POST", "/events", Some(&t), Some(json!({"events": [
        {"clientId": "01COLDEVENT000000000000001", "type": "lecture.focus", "occurredAt": ago, "componentId": c.l1, "payload": {"seconds": 60}}
    ]})), &[]).await;
    assert_eq!(ev.json["accepted"], 1, "{}", ev.json);
    assert_eq!(call(&h.app, "POST", "/attempts", Some(&t), Some(practice(1)), &[]).await.status, StatusCode::OK);
    let view = call(&h.app, "GET", &format!("/courses/{}/weak-spots", c.id), Some(&t), None, &[]).await.json;
    assert_eq!(view["open"][0]["topic"], "Median", "studied in the last 24 h → not a cold pass: {view}");

    // A day and a half later, nothing studied since: cold, and one cold pass resolves it.
    h.t.clock.advance(chrono::Duration::hours(36));
    let t = call(&h.app, "POST", "/auth/login", None, Some(json!({"login": "reader", "password": "correct horse battery"})), &[])
        .await
        .json["accessToken"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(call(&h.app, "POST", "/attempts", Some(&t), Some(practice(2)), &[]).await.status, StatusCode::OK);
    let view = call(&h.app, "GET", &format!("/courses/{}/weak-spots", c.id), Some(&t), None, &[]).await.json;
    assert_eq!(view["resolved"][0]["topic"], "Median", "{view}");
    h.db.drop_db().await;
}
