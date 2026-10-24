//! Milestone 5: events, derived sessions, deadlines, plan, /next, warm-up, week, check-ins (spec 5.7-5.11).

mod common;

use axum::http::StatusCode;
use chrono::{Duration, TimeZone, Utc};
use crux_testkit::{call, outbox_topics, signup};
use serde_json::{json, Value};

fn cid(n: u32) -> String {
    format!("01EVENT{n:019}")
}

fn at(h: &common::Harness, d: Duration) -> String {
    crux_service::time::iso(h.t.ctx.clock.now() + d)
}

async fn login(h: &common::Harness, name: &str) -> String {
    let r = call(&h.app, "POST", "/auth/login", None, Some(json!({"login": name, "password": "correct horse battery"})), &[]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    r.json["accessToken"].as_str().unwrap().to_string()
}

async fn ok(h: &common::Harness, method: &str, path: &str, token: &str, body: Option<Value>, headers: &[(&str, &str)]) -> Value {
    let r = call(&h.app, method, path, Some(token), body, headers).await;
    assert!(r.status.is_success(), "{method} {path}: {} {}", r.status, r.json);
    r.json
}

#[tokio::test]
async fn plan_next_sessions_week_and_check_ins() {
    let Some(h) = common::harness().await else { return };
    // The test clock starts Fri 23 Oct 2026 12:00 UTC (14:00 in Amsterdam).
    let (t, _, _) = signup(&h.app, &h.t.ctx, "nina").await;
    let course = ok(&h, "POST", "/courses", &t, Some(json!({"name": "Business stats", "code": "BSA"})), &[]).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut lectures = Vec::new();
    for (code, hours) in [("L1", 2.0), ("L2", 2.0), ("L3", 1.0)] {
        let k =
            ok(&h, "POST", &format!("/courses/{course}/components"), &t, Some(json!({"code": code, "name": code, "estHours": hours})), &[])
                .await;
        lectures.push(k["id"].as_str().unwrap().to_string());
    }

    // An exam: Tue 3 Nov 09:00 Amsterdam. The plan builds itself.
    let exam = ok(
        &h,
        "POST",
        &format!("/courses/{course}/deadlines"),
        &t,
        Some(json!({"type": "Exam", "title": "Final", "startsAt": "2026-11-03T08:00:00Z", "durationMin": 90})),
        &[],
    )
    .await;
    assert_eq!((exam["status"].as_str(), exam["awaitingResult"].as_bool()), (Some("Upcoming"), Some(false)));
    let plan = ok(&h, "GET", &format!("/plan?from={}&to=2026-11-04T00:00:00Z", at(&h, Duration::zero())), &t, None, &[]).await;
    let entries = plan.as_array().unwrap();
    let kinds: Vec<&str> = entries.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"Lecture") && kinds.contains(&"Mock") && kinds.contains(&"Revision"), "{kinds:?}");
    let mock = entries.iter().find(|e| e["kind"] == "Mock").unwrap();
    assert!(mock["startAt"].as_str().unwrap().starts_with("2026-11-01"), "timed mock 2 days before: {mock}");
    assert!(entries.iter().all(|e| e["endAt"].as_str().unwrap() < "2026-11-03T08:00:00Z"), "nothing on exam morning");
    let lecture_hours: f64 = entries
        .iter()
        .filter(|e| e["kind"] == "Lecture")
        .map(|e| {
            let s = chrono::DateTime::parse_from_rfc3339(e["startAt"].as_str().unwrap()).unwrap();
            let f = chrono::DateTime::parse_from_rfc3339(e["endAt"].as_str().unwrap()).unwrap();
            (f - s).num_minutes() as f64 / 60.0
        })
        .sum();
    assert!((lecture_hours - 5.0).abs() < 0.01, "all 5 lecture hours planned: {lecture_hours}");

    let again = ok(&h, "POST", "/plan/rebuild", &t, None, &[]).await;
    assert!(again["changeSummary"].as_str().unwrap().starts_with("No changes"), "{}", again["changeSummary"]);

    // Moving an entry pins it; the old one is Moved and points at the new one.
    let first = entries.iter().find(|e| e["kind"] == "Lecture").unwrap();
    let moved_to = at(&h, Duration::days(1));
    let moved = ok(
        &h,
        "PATCH",
        &format!("/plan/entries/{}", first["id"].as_str().unwrap()),
        &t,
        Some(json!({"startAt": moved_to})),
        &[("if-match", &first["version"].to_string())],
    )
    .await;
    assert_eq!((moved["pinned"].as_bool(), moved["status"].as_str()), (Some(true), Some("Planned")));
    let rebuilt = ok(&h, "POST", "/plan/rebuild", &t, None, &[]).await;
    assert!(rebuilt["changeSummary"].as_str().unwrap().contains("1 pinned kept"), "{}", rebuilt["changeSummary"]);
    assert!(rebuilt["entries"].as_array().unwrap().iter().any(|e| e["id"] == moved["id"]), "the pinned entry survives");

    // Events: accepted, duplicate, unknown type, server-only type.
    let b = ok(
        &h,
        "POST",
        "/events",
        &t,
        Some(json!({"events": [
            {"clientId": cid(1), "type": "material.page", "occurredAt": at(&h, -Duration::minutes(50)), "componentId": lectures[0], "payload": {"page": 1}},
            {"clientId": cid(2), "type": "material.page", "occurredAt": at(&h, -Duration::minutes(40)), "componentId": lectures[0], "payload": {"page": 2}},
            {"clientId": cid(3), "type": "notes.step", "occurredAt": at(&h, -Duration::minutes(35)), "courseId": course, "componentId": lectures[0], "payload": {"step": 3}},
            {"clientId": cid(3), "type": "notes.step", "occurredAt": at(&h, -Duration::minutes(35)), "courseId": course, "payload": {"step": 3}},
            {"clientId": cid(4), "type": "nope.nothing", "occurredAt": at(&h, Duration::zero())},
            {"clientId": cid(5), "type": "mock.start", "occurredAt": at(&h, Duration::zero()), "courseId": course},
            {"clientId": cid(6), "type": "material.page", "occurredAt": at(&h, Duration::zero()), "courseId": course, "payload": {"page": 0}}
        ]})),
        &[],
    )
    .await;
    assert_eq!((b["accepted"].as_i64(), b["duplicates"].as_i64()), (Some(3), Some(1)), "{b}");
    let reasons: Vec<&str> = b["rejected"].as_array().unwrap().iter().map(|r| r["reason"].as_str().unwrap()).collect();
    assert_eq!(reasons, ["unknown_type", "server_only", "bad_payload"]);

    // Derived session: first event to last + 2 min, on L1, kind Lecture.
    let sessions = ok(&h, "GET", "/sessions", &t, None, &[]).await;
    let derived: Vec<&Value> = sessions.as_array().unwrap().iter().filter(|s| s["source"] == "Derived").collect();
    assert_eq!(derived.len(), 1, "{sessions}");
    assert_eq!(
        (derived[0]["minutes"].as_i64(), derived[0]["kind"].as_str(), derived[0]["componentId"].as_str()),
        (Some(17), Some("Lecture"), Some(lectures[0].as_str()))
    );
    // A manual session over the same time wins.
    let manual = call(
        &h.app,
        "POST",
        "/sessions",
        Some(&t),
        Some(json!({"courseId": course, "componentId": lectures[0], "kind": "Lecture", "startedAt": at(&h, -Duration::minutes(60)), "endedAt": at(&h, -Duration::minutes(30)), "source": "Manual"})),
        &[],
    )
    .await;
    assert_eq!(manual.status, StatusCode::CREATED, "{}", manual.json);
    let sessions = ok(&h, "GET", "/sessions", &t, None, &[]).await;
    let sources: Vec<&str> = sessions.as_array().unwrap().iter().map(|s| s["source"].as_str().unwrap()).collect();
    assert_eq!(sources, ["Manual"], "the derived one is dropped");

    // Finishing L1: progress, plan, summary.
    let p =
        ok(&h, "PATCH", &format!("/courses/{course}/progress"), &t, Some(json!({"componentId": lectures[0], "status": "Done"})), &[]).await;
    assert_eq!((p["status"].as_str(), p["actualHours"].as_f64()), (Some("Done"), Some(0.5)));
    let s = ok(&h, "GET", &format!("/courses/{course}/stats/summary"), &t, None, &[]).await;
    assert_eq!((s["lecturesDone"].as_i64(), s["lecturesTotal"].as_i64()), (Some(1), Some(3)));
    assert_eq!(
        (s["remainingHours"]["low"].as_f64(), s["remainingHours"]["mid"].as_f64(), s["remainingHours"]["high"].as_f64()),
        (Some(2.3), Some(3.0), Some(3.8)),
        "{s}"
    );
    assert_eq!(s["nextExam"]["daysLeft"], 11);

    // Warm-up: a flagged card first, then one question per open weak spot.
    let concept = ok(&h, "POST", &format!("/courses/{course}/concepts"), &t, Some(json!({"name": "Variance"})), &[]).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    ok(
        &h,
        "POST",
        &format!("/courses/{course}/questions"),
        &t,
        Some(json!({
            "componentId": lectures[1], "type": "MC", "conceptIds": [concept], "primaryConceptId": concept,
            "content": {"prompt": "Var(aX)?", "options": [{"text": "a²Var(X)", "isCorrect": true}, {"text": "aVar(X)", "isCorrect": false}]}
        })),
        &[],
    )
    .await;
    ok(
        &h,
        "POST",
        &format!("/courses/{course}/weak-spots"),
        &t,
        Some(json!({"topic": "Variance rules", "conceptId": concept, "severity": "High"})),
        &[],
    )
    .await;
    let card = ok(
        &h,
        "POST",
        &format!("/courses/{course}/flashcards"),
        &t,
        Some(json!({"componentId": lectures[1], "front": "SD?", "back": "√Var"})),
        &[],
    )
    .await;
    ok(&h, "POST", &format!("/flashcards/{}/warmup-flag", card["id"].as_str().unwrap()), &t, Some(json!({"flagged": true})), &[]).await;
    let w = ok(&h, "GET", &format!("/courses/{course}/warmup"), &t, None, &[]).await;
    let kinds: Vec<&str> = w.as_array().unwrap().iter().map(|i| i["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["card", "question"], "{w}");
    assert!(w[1]["question"]["options"][0].get("isCorrect").is_none(), "no key in the warm-up");

    // /next: warm-up, then today's plan entry; the reason names the exam.
    let n = ok(&h, "GET", "/next", &t, None, &[]).await;
    assert_eq!(n["course"]["code"], "BSA");
    assert!(n["reason"].as_str().unwrap().starts_with("BSA exam in 11 days"), "{}", n["reason"]);
    assert!(n["reason"].as_str().unwrap().contains("1 open weak spot"), "{}", n["reason"]);
    let steps = n["steps"].as_array().unwrap();
    assert_eq!(steps[0]["kind"], "warmup", "{n}");
    let main = &steps[1];
    assert!(main["planEntryId"].is_string(), "today's plan entry: {n}");
    let r = call(
        &h.app,
        "POST",
        "/next/advance",
        Some(&t),
        Some(json!({"stepIndex": 1, "kind": main["kind"], "planEntryId": main["planEntryId"]})),
        &[],
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.json);
    let today = ok(&h, "GET", "/plan", &t, None, &[]).await;
    assert!(today.as_array().unwrap().iter().any(|e| e["id"] == main["planEntryId"] && e["status"] == "Done"));

    // The week read model.
    let week = ok(&h, "GET", "/week", &t, None, &[]).await;
    assert_eq!(week["isoWeek"], "2026-W43");
    let sum = &week["summary"];
    assert_eq!(sum["days"].as_array().unwrap().len(), 7);
    assert_eq!((sum["totals"]["actualHours"].as_f64(), sum["totals"]["stepsDone"].as_i64()), (Some(0.5), Some(1)), "{sum}");
    assert_eq!(sum["courses"][0]["code"], "BSA");

    // Reminders: eve at 18:00 local the day before, check-in at 17:00 on the day; once each.
    let quiz = ok(
        &h,
        "POST",
        &format!("/courses/{course}/deadlines"),
        &t,
        Some(json!({"type": "Exam", "title": "Midterm", "startsAt": "2026-10-24T08:00:00Z"})),
        &[],
    )
    .await;
    let notify_count = |topics: &[String]| topics.iter().filter(|t| *t == "notify.requested").count();
    let before = notify_count(&outbox_topics(&h.db.pool).await);
    h.t.clock.set(Utc.with_ymd_and_hms(2026, 10, 23, 16, 5, 0).unwrap()); // 18:05 Amsterdam
    assert!(crux_service::deadlines::tick(&h.t.ctx).await.unwrap() >= 1);
    crux_service::deadlines::tick(&h.t.ctx).await.unwrap();
    assert_eq!(notify_count(&outbox_topics(&h.db.pool).await), before + 1, "eve requested once");
    h.t.clock.set(Utc.with_ymd_and_hms(2026, 10, 24, 15, 5, 0).unwrap()); // 17:05 on exam day
    crux_service::deadlines::tick(&h.t.ctx).await.unwrap();
    assert_eq!(notify_count(&outbox_topics(&h.db.pool).await), before + 2, "check-in requested");

    // Behind: yesterday's unfinished planner entries are skipped and the plan rebuilt.
    let t = login(&h, "nina").await;
    let skipped = ok(&h, "GET", "/plan?from=2026-10-23T00:00:00Z&to=2026-10-24T00:00:00Z", &t, None, &[]).await;
    assert!(skipped.as_array().unwrap().iter().any(|e| e["status"] == "Skipped"), "{skipped}");

    // After the exam: "how did it go?", then a fail offers a resit, a pass archives the course.
    let n = ok(&h, "GET", "/next", &t, None, &[]).await;
    assert_eq!(n["checkIns"].as_array().unwrap().len(), 1, "{n}");
    let qid = quiz["id"].as_str().unwrap();
    let failed = ok(
        &h,
        "PATCH",
        &format!("/deadlines/{qid}"),
        &t,
        Some(json!({"status": "Done", "gradeValue": 40})),
        &[("if-match", &quiz["version"].to_string())],
    )
    .await;
    assert_eq!((failed["passed"].as_bool(), failed["offerResit"].as_bool()), (Some(false), Some(true)), "{failed}");
    let resit = ok(
        &h,
        "POST",
        &format!("/courses/{course}/deadlines"),
        &t,
        Some(json!({"type": "Resit", "resitOfId": qid, "startsAt": "2026-11-20T08:00:00Z"})),
        &[],
    )
    .await;
    assert_eq!(resit["resitOfId"].as_str(), Some(qid));
    let list = ok(&h, "GET", &format!("/courses/{course}/deadlines"), &t, None, &[]).await;
    assert!(list.as_array().unwrap().iter().all(|d| d["offerResit"] == false), "resit planned");
    let fid = exam["id"].as_str().unwrap();
    let bad = call(&h.app, "PATCH", &format!("/deadlines/{fid}"), Some(&t), Some(json!({"gradeValue": 70})), &[("if-match", "0")]).await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY, "a grade needs Done");
    let passed = ok(
        &h,
        "PATCH",
        &format!("/deadlines/{fid}"),
        &t,
        Some(json!({"status": "Done", "gradeValue": 72, "checkInNote": "Went fine"})),
        &[("if-match", &exam["version"].to_string())],
    )
    .await;
    assert_eq!(passed["passed"], true);
    let c = ok(&h, "GET", &format!("/courses/{course}"), &t, None, &[]).await;
    assert_eq!(c["mySubscription"], "Passed");
    let cards = ok(&h, "GET", &format!("/courses/{course}/flashcards"), &t, None, &[]).await;
    assert_eq!(cards[0]["progress"]["suspendReason"], "CourseArchived");
    let n = ok(&h, "GET", "/next", &t, None, &[]).await;
    assert!(n["course"].is_null(), "a passed course leaves /next: {n}");
    let p = ok(&h, "GET", "/plan?from=2026-10-24T16:00:00Z&to=2026-11-10T00:00:00Z", &t, None, &[]).await;
    assert!(p.as_array().unwrap().iter().all(|e| e["createdBy"] != "Planner" || e["status"] != "Planned"), "out of the plan: {p}");

    // Purging a course with all of this in it.
    ok(&h, "POST", &format!("/courses/{course}/mocks"), &t, Some(json!({})), &[]).await;
    ok(&h, "POST", "/slips", &t, Some(json!({"courseId": course, "type": "Sign", "description": "minus"})), &[]).await;
    let r = call(&h.app, "DELETE", &format!("/courses/{course}"), Some(&t), None, &[]).await;
    assert!(r.status.is_success(), "{}", r.json);
    h.t.clock.advance(Duration::days(31));
    assert_eq!(crux_service::courses::purge_trashed(&h.t.ctx).await.unwrap(), 1);
    h.db.drop_db().await;
}

/// (kind, dedupeKey, cancel) of every notify.requested in the outbox, in order.
async fn notify_requests(pool: &sqlx::MySqlPool) -> Vec<(String, String, bool)> {
    sqlx::query_as(
        "SELECT CAST(JSON_UNQUOTE(JSON_EXTRACT(Payload, '$.kind')) AS CHAR), CAST(JSON_UNQUOTE(JSON_EXTRACT(Payload, '$.dedupeKey')) AS CHAR), \
         COALESCE(JSON_EXTRACT(Payload, '$.cancel') = TRUE, FALSE) FROM Outbox WHERE Topic = 'notify.requested' \
         AND JSON_UNQUOTE(JSON_EXTRACT(Payload, '$.kind')) NOT LIKE 'auth.%' ORDER BY Id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn digests_and_reminder_changes() {
    let Some(h) = common::harness().await else { return };
    // Fri 23 Oct 2026 05:00 UTC = 07:00 Amsterdam.
    h.t.clock.set(Utc.with_ymd_and_hms(2026, 10, 23, 5, 0, 0).unwrap());
    let (t, _, _) = signup(&h.app, &h.t.ctx, "dora").await;
    let course =
        ok(&h, "POST", "/courses", &t, Some(json!({"name": "Stats", "code": "ST"})), &[]).await["id"].as_str().unwrap().to_string();
    let l1 = ok(&h, "POST", &format!("/courses/{course}/components"), &t, Some(json!({"code": "L1", "name": "Intro", "estHours": 1})), &[])
        .await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let card =
        ok(&h, "POST", &format!("/courses/{course}/flashcards"), &t, Some(json!({"componentId": l1, "front": "F", "back": "B"})), &[])
            .await;
    let review = json!({"reviews": [{"clientId": "01REVIEW000000000000000001", "flashcardId": card["id"], "rating": 3, "reviewedAt": "2026-10-20T08:00:00Z"}]});
    ok(&h, "POST", "/flash/reviews", &t, Some(review), &[]).await;
    let exam =
        ok(&h, "POST", &format!("/courses/{course}/deadlines"), &t, Some(json!({"type": "Exam", "startsAt": "2026-10-24T08:00:00Z"})), &[])
            .await;

    let kinds = |v: &[(String, String, bool)]| v.iter().map(|r| r.0.clone()).collect::<Vec<_>>();
    crux_service::deadlines::tick(&h.t.ctx).await.unwrap();
    assert!(kinds(&notify_requests(&h.db.pool).await).is_empty(), "07:00: nothing yet");
    h.t.clock.set(Utc.with_ymd_and_hms(2026, 10, 23, 6, 5, 0).unwrap()); // 08:05
    crux_service::deadlines::tick(&h.t.ctx).await.unwrap();
    crux_service::deadlines::tick(&h.t.ctx).await.unwrap();
    let got = notify_requests(&h.db.pool).await;
    assert_eq!(kinds(&got), ["plan.today", "flash.due"], "once each: {got:?}");
    assert_eq!(got[1].1, "flash.due:2026-10-23");

    // 18:05: the eve reminder; then the exam moves: the reminder is re-requested with the same key.
    h.t.clock.set(Utc.with_ymd_and_hms(2026, 10, 23, 16, 5, 0).unwrap());
    crux_service::deadlines::tick(&h.t.ctx).await.unwrap();
    let t = login(&h, "dora").await;
    let eid = exam["id"].as_str().unwrap();
    let moved = ok(
        &h,
        "PATCH",
        &format!("/deadlines/{eid}"),
        &t,
        Some(json!({"startsAt": "2026-10-26T08:00:00Z"})),
        &[("if-match", &exam["version"].to_string())],
    )
    .await;
    let got = notify_requests(&h.db.pool).await;
    let eve_key = format!("exam.eve:{eid}");
    assert_eq!(got.iter().filter(|r| r.1 == eve_key && !r.2).count(), 2, "requested, then re-requested: {got:?}");
    // Cancelled: both reminders are cancelled by key.
    ok(&h, "PATCH", &format!("/deadlines/{eid}"), &t, Some(json!({"status": "Cancelled"})), &[("if-match", &moved["version"].to_string())])
        .await;
    let got = notify_requests(&h.db.pool).await;
    assert_eq!(got.iter().filter(|r| r.2).map(|r| r.0.as_str()).collect::<Vec<_>>(), ["exam.eve", "exam.checkin"]);
    h.db.drop_db().await;
}

#[tokio::test]
async fn the_study_window_setting_re_lays_the_plan() {
    let Some(h) = common::harness().await else { return };
    let (t, _, _) = signup(&h.app, &h.t.ctx, "wendy").await;
    let course = ok(&h, "POST", "/courses", &t, Some(json!({"name": "Evening course", "code": "EVE"})), &[]).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    ok(&h, "POST", &format!("/courses/{course}/components"), &t, Some(json!({"code": "L1", "name": "L1", "estHours": 3.0})), &[]).await;
    ok(&h, "POST", &format!("/courses/{course}/deadlines"), &t, Some(json!({"type": "Exam", "startsAt": "2026-11-20T08:00:00Z"})), &[])
        .await;

    let bad = call(&h.app, "GET", "/me/settings", Some(&t), None, &[]).await;
    let v = bad.json["version"].as_i64().unwrap().to_string();
    let r = call(&h.app, "PATCH", "/me/settings", Some(&t), Some(json!({"extra": {"studyStart": "21:30"}})), &[("if-match", &v)]).await;
    assert_eq!(
        (r.status, r.json["error"]["details"]["fields"][0]["field"].as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("extra.studyEnd"))
    );

    // 19:00-21:00 Amsterdam.
    ok(&h, "PATCH", "/me/settings", &t, Some(json!({"extra": {"studyStart": "19:00", "studyEnd": "21:00"}})), &[("if-match", &v)]).await;
    let plan = ok(&h, "GET", &format!("/plan?from={}&to=2026-11-21T00:00:00Z", at(&h, Duration::zero())), &t, None, &[]).await;
    let lectures: Vec<&Value> = plan.as_array().unwrap().iter().filter(|e| e["kind"] == "Lecture").collect();
    assert!(!lectures.is_empty());
    // Amsterdam is UTC+2 until 25 Oct 01:00 UTC, then UTC+1.
    let local = |iso: &str| {
        let t = chrono::DateTime::parse_from_rfc3339(iso).unwrap().with_timezone(&Utc);
        let off = if t < Utc.with_ymd_and_hms(2026, 10, 25, 1, 0, 0).unwrap() { 2 } else { 1 };
        (t + Duration::hours(off)).format("%H:%M").to_string()
    };
    for e in &lectures {
        let (s, f) = (local(e["startAt"].as_str().unwrap()), local(e["endAt"].as_str().unwrap()));
        assert!(s.as_str() >= "19:00" && f.as_str() <= "21:00", "{s}-{f}: {e}");
    }
    h.db.drop_db().await;
}
