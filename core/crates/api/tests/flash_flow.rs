//! Milestone 4: flashcards, FSRS reviews, queue, custom sets, archive suspension (spec 5.5).

mod common;

use axum::http::StatusCode;
use chrono::Duration;
use crux_testkit::{call, signup};
use serde_json::{json, Value};

fn cid(n: u32) -> String {
    format!("01REVIEW{n:018}")
}

fn ago(h: &common::Harness, d: Duration) -> String {
    crux_service::time::iso(h.t.ctx.clock.now() - d)
}

async fn review(h: &common::Harness, token: &str, reviews: Value) -> Value {
    let r = call(&h.app, "POST", "/flash/reviews", Some(token), Some(json!({ "reviews": reviews })), &[]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    r.json
}

#[tokio::test]
async fn cards_reviews_queue_and_archive() {
    let Some(h) = common::harness().await else { return };
    let (owner, _, _) = signup(&h.app, &h.t.ctx, "owner").await;
    let (vera, _, _) = signup(&h.app, &h.t.ctx, "vera").await;
    let c = call(&h.app, "POST", "/courses", Some(&owner), Some(json!({"name": "Business stats", "code": "BSA"})), &[]).await.json;
    let course = c["id"].as_str().unwrap().to_string();
    let l1 =
        call(&h.app, "POST", &format!("/courses/{course}/components"), Some(&owner), Some(json!({"code": "L1", "name": "Intro"})), &[])
            .await
            .json["id"]
            .as_str()
            .unwrap()
            .to_string();
    call(
        &h.app,
        "POST",
        &format!("/courses/{course}/members"),
        Some(&owner),
        Some(json!({"usernameOrEmail": "vera", "role": "Viewer"})),
        &[],
    )
    .await;
    call(&h.app, "POST", &format!("/course-invitations/{course}/accept"), Some(&vera), None, &[]).await;
    let sub = call(&h.app, "POST", &format!("/courses/{course}/subscription"), Some(&vera), Some(json!({})), &[]).await;
    assert_eq!(sub.status, StatusCode::OK, "{}", sub.json);

    // Codes: per course, in order. A Viewer may make personal cards but not course cards.
    let shared = call(
        &h.app,
        "POST",
        &format!("/courses/{course}/flashcards"),
        Some(&owner),
        Some(json!({"componentId": l1, "front": "Mean?", "back": "Sum / n", "shared": true})),
        &[],
    )
    .await;
    assert_eq!((shared.status, shared.json["code"].as_str()), (StatusCode::CREATED, Some("BSA001")), "{}", shared.json);
    let card1 = shared.json["id"].as_str().unwrap().to_string();
    let mine = call(
        &h.app,
        "POST",
        &format!("/courses/{course}/flashcards"),
        Some(&vera),
        Some(json!({"componentId": l1, "front": "Median?", "back": "Middle value"})),
        &[],
    )
    .await;
    assert_eq!((mine.json["code"].as_str(), mine.json["personal"].as_bool()), (Some("BSA002"), Some(true)), "{}", mine.json);
    let card2 = mine.json["id"].as_str().unwrap().to_string();
    let refused = call(
        &h.app,
        "POST",
        &format!("/courses/{course}/flashcards"),
        Some(&vera),
        Some(json!({"componentId": l1, "front": "x", "back": "y", "shared": true})),
        &[],
    )
    .await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "a Viewer can't add course cards");

    let owner_list = call(&h.app, "GET", &format!("/courses/{course}/flashcards"), Some(&owner), None, &[]).await.json;
    assert_eq!(owner_list.as_array().unwrap().len(), 1, "vera's personal card is hers alone");
    let vera_list = call(&h.app, "GET", &format!("/courses/{course}/flashcards"), Some(&vera), None, &[]).await.json;
    assert_eq!(vera_list.as_array().unwrap().len(), 2);
    let found = call(&h.app, "GET", "/flash/cards?code=BSA002", Some(&vera), None, &[]).await.json;
    assert_eq!(found.as_array().unwrap().len(), 1);
    let hidden = call(&h.app, "GET", "/flash/cards?code=BSA002", Some(&owner), None, &[]).await.json;
    assert_eq!(hidden.as_array().unwrap().len(), 0);

    // Personal cards: only their owner edits them (the course owner gets 404).
    let p = call(&h.app, "PATCH", &format!("/flashcards/{card2}"), Some(&owner), Some(json!({"back": "x"})), &[("if-match", "0")]).await;
    assert_eq!(p.status, StatusCode::NOT_FOUND);
    let p = call(
        &h.app,
        "PATCH",
        &format!("/flashcards/{card2}"),
        Some(&vera),
        Some(json!({"back": "The middle value"})),
        &[("if-match", "0")],
    )
    .await;
    assert_eq!((p.status, p.json["version"].as_i64()), (StatusCode::OK, Some(1)), "{}", p.json);

    // Queue: two new cards.
    let q = call(&h.app, "GET", "/flash/queue", Some(&vera), None, &[]).await.json;
    assert_eq!((q["dueCount"].as_i64(), q["newCount"].as_i64()), (Some(0), Some(2)), "{q}");
    // Each card carries the intervals its rating buttons show: Again soonest, Easy latest.
    let next = q["nextDue"].as_array().unwrap();
    assert_eq!(next.len(), 2, "{q}");
    let n = &next[0];
    let secs = |k: &str| n[k].as_i64().unwrap();
    assert!(secs("again") < secs("good") && secs("good") <= secs("easy") && secs("hard") <= secs("good"), "{n}");
    assert!(secs("again") > 0 && secs("again") < 3600, "a new card's Again comes back within the hour: {n}");

    // Offline batch, sent out of order: Good 10 days ago, then Again (Sure) yesterday = a lapse.
    let b = review(
        &h,
        &vera,
        json!([
            {"clientId": cid(2), "flashcardId": card1, "rating": 1, "confidence": "Sure", "reviewedAt": ago(&h, Duration::days(1)), "durationMs": 4000},
            {"clientId": cid(1), "flashcardId": card1, "rating": 3, "reviewedAt": ago(&h, Duration::days(10))},
            {"clientId": cid(3), "flashcardId": "01AAAAAAAAAAAAAAAAAAAAAAAA", "rating": 3, "reviewedAt": ago(&h, Duration::days(1))}
        ]),
    )
    .await;
    let statuses: Vec<_> = b["results"].as_array().unwrap().iter().map(|r| r["status"].as_str().unwrap().to_string()).collect();
    assert_eq!(statuses, ["applied", "applied", "unknown"], "applied in reviewedAt order: {b}");
    assert_eq!(b["results"][1]["card"]["progress"]["state"], "Relearning");
    assert_eq!(b["results"][1]["card"]["progress"]["lapses"], 1);
    assert_eq!(b["slipProposals"].as_array().unwrap().len(), 1, "{b}");
    assert_eq!(b["slipProposals"][0]["reason"], "lapse");

    // Retried and late reviews.
    let again = review(
        &h,
        &vera,
        json!([
            {"clientId": cid(2), "flashcardId": card1, "rating": 1, "reviewedAt": ago(&h, Duration::days(1))},
            {"clientId": cid(4), "flashcardId": card1, "rating": 4, "reviewedAt": ago(&h, Duration::days(20))}
        ]),
    )
    .await;
    let statuses: Vec<_> = again["results"].as_array().unwrap().iter().map(|r| r["status"].as_str().unwrap().to_string()).collect();
    assert_eq!(statuses, ["stale", "duplicate"], "{again}");
    assert_eq!(again["slipProposals"].as_array().unwrap().len(), 0);

    // Confirm the proposal as a slip, sourced from the review.
    let slip = call(
        &h.app,
        "POST",
        "/slips",
        Some(&vera),
        Some(json!({"courseId": course, "type": "Concept", "description": "Forgot the mean", "flashcardReviewClientId": cid(2)})),
        &[],
    )
    .await;
    assert_eq!((slip.status, slip.json["componentId"].as_str()), (StatusCode::CREATED, Some(l1.as_str())), "{}", slip.json);
    let foreign = call(
        &h.app,
        "POST",
        "/slips",
        Some(&owner),
        Some(json!({"courseId": course, "type": "Concept", "description": "x", "flashcardReviewClientId": cid(2)})),
        &[],
    )
    .await;
    assert_eq!(foreign.status, StatusCode::NOT_FOUND, "someone else's review");

    // Queue: card1 is due again (relearning), card2 still new; nothing new was seen today.
    let q = call(&h.app, "GET", "/flash/queue", Some(&vera), None, &[]).await.json;
    assert_eq!((q["dueCount"].as_i64(), q["newCount"].as_i64(), q["newLeftToday"].as_i64()), (Some(1), Some(1), Some(19)), "{q}");
    let today =
        review(&h, &vera, json!([{"clientId": cid(5), "flashcardId": card2, "rating": 3, "reviewedAt": ago(&h, Duration::minutes(1))}]))
            .await;
    assert_eq!(today["results"][0]["card"]["progress"]["state"], "Review");
    let q = call(&h.app, "GET", "/flash/queue", Some(&vera), None, &[]).await.json;
    assert_eq!((q["newCount"].as_i64(), q["newLeftToday"].as_i64()), (Some(0), Some(19)), "{q}");

    // Custom set: forgotten cards.
    let set = call(&h.app, "POST", "/flash/custom", Some(&vera), Some(json!({"courseId": course, "which": "forgotten"})), &[]).await;
    assert_eq!(set.json.as_array().map(|a| a.len()), Some(1), "{}", set.json);
    let bad = call(&h.app, "POST", "/flash/custom", Some(&vera), Some(json!({"courseId": course, "which": "everything"})), &[]).await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);

    // Archiving suspends; un-archiving lifts only that suspension, not a manual one.
    let s = call(&h.app, "POST", &format!("/flashcards/{card2}/suspend"), Some(&vera), Some(json!({"suspended": true})), &[]).await;
    assert_eq!(s.json["progress"]["suspendReason"], "Manual");
    call(&h.app, "POST", &format!("/courses/{course}/subscription"), Some(&vera), Some(json!({"status": "Archived"})), &[]).await;
    let list = call(&h.app, "GET", &format!("/courses/{course}/flashcards"), Some(&vera), None, &[]).await.json;
    let reasons: Vec<_> =
        list.as_array().unwrap().iter().map(|c| c["progress"]["suspendReason"].as_str().unwrap_or("-").to_string()).collect();
    assert_eq!(reasons, ["CourseArchived", "Manual"]);
    let q = call(&h.app, "GET", "/flash/queue", Some(&vera), None, &[]).await.json;
    assert_eq!(q["cards"].as_array().unwrap().len(), 0, "archived courses leave the queue");
    call(&h.app, "POST", &format!("/courses/{course}/subscription"), Some(&vera), Some(json!({"status": "Active"})), &[]).await;
    let list = call(&h.app, "GET", &format!("/courses/{course}/flashcards"), Some(&vera), None, &[]).await.json;
    let suspended: Vec<_> = list.as_array().unwrap().iter().map(|c| c["progress"]["suspended"].as_bool().unwrap()).collect();
    assert_eq!(suspended, [false, true]);

    // Warm-up flag.
    let f = call(&h.app, "POST", &format!("/flashcards/{card1}/warmup-flag"), Some(&vera), Some(json!({"flagged": true})), &[]).await;
    assert_eq!(f.json["progress"]["warmupFlagged"], true);

    // Validation.
    let v = call(&h.app, "POST", "/flash/reviews", Some(&vera), Some(json!({"reviews": [
        {"clientId": "short", "flashcardId": card1, "rating": 7, "reviewedAt": crux_service::time::iso(h.t.ctx.clock.now() + Duration::hours(1))}
    ]})), &[]).await;
    assert_eq!(v.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(v.json["error"]["details"]["fields"].as_array().unwrap().len(), 3, "{}", v.json);
    h.db.drop_db().await;
}
