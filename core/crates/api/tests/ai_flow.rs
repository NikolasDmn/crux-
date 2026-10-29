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

#[tokio::test]
async fn generated_questions_and_topup_plan() {
    let Some(h) = common::harness().await else { return };
    let (t, me, _) = signup(&h.app, &h.t.ctx, "gina").await;
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
    let svc = h.t.ctx.keys.sign_service("ai", "core", h.t.ctx.clock.now().timestamp());
    let auth = format!("Bearer {svc}");
    let headers = [("authorization", auth.as_str())];
    let opts = |two: bool| json!([{"text": "4 Var(X)", "isCorrect": true}, {"text": "2 Var(X)", "isCorrect": two}]);
    let body = |user: &str| {
        json!({
            "user": user, "course": course, "component": l1, "autoApprove": false,
            "questions": [
                {"type": "MC", "content": {"prompt": "What is Var(2X)?", "keyReliability": "Official", "options": opts(false)}},
                {"type": "MC", "content": {"prompt": "Two right?", "options": opts(true)}}
            ]
        })
    };
    let r = call(&h.app, "POST", "/internal/v1/questions/generated", None, Some(body(&me)), &headers).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!((r.json["created"].as_array().unwrap().len(), r.json["rejected"].as_array().unwrap().len()), (1, 1));
    let qid = r.json["created"][0].as_str().unwrap();
    let q = call(&h.app, "GET", &format!("/questions/{qid}"), Some(&t), None, &[]).await.json;
    assert_eq!(
        (q["source"].as_str(), q["private"].as_bool(), q["reviewState"].as_str()),
        (Some("Generated"), Some(true), Some("Draft")),
        "{q}"
    );
    assert_eq!(q["current"]["keyReliability"], "Derived", "never Official");
    let r = call(&h.app, "POST", "/internal/v1/questions/generated", None, Some(body(&bob)), &headers).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND, "core re-checks access for the user");

    call(
        &h.app,
        "POST",
        &format!("/courses/{course}/deadlines"),
        Some(&t),
        Some(json!({"type": "Exam", "startsAt": "2026-11-10T09:00:00Z"})),
        &[],
    )
    .await;
    let r = call(&h.app, "GET", &format!("/internal/v1/ai/topup-plan?user={me}"), None, None, &headers).await;
    assert_eq!(
        (r.json["timeZone"].as_str(), r.json["courses"][0]["courseId"].as_str()),
        (Some("Europe/Amsterdam"), Some(course.as_str())),
        "{}",
        r.json
    );
    assert_eq!(r.json["courses"][0]["daysToExam"], 17);
    let r =
        call(&h.app, "GET", &format!("/internal/v1/ai/generation-context?user={me}&course={course}&component={l1}"), None, None, &headers)
            .await;
    assert_eq!(r.json["existingPrompts"][0], "What is Var(2X)?");
    h.db.drop_db().await;
}

#[tokio::test]
async fn import_apply_is_ordered_checked_and_idempotent() {
    let Some(h) = common::harness().await else { return };
    let (t, me, _) = signup(&h.app, &h.t.ctx, "ivy").await;
    let me = me["id"].as_str().unwrap().to_string();
    let (bt, bob, _) = signup(&h.app, &h.t.ctx, "bob").await;
    let bob = bob["id"].as_str().unwrap().to_string();
    let svc = h.t.ctx.keys.sign_service("ai", "core", h.t.ctx.clock.now().timestamp());
    let auth = format!("Bearer {svc}");
    let headers = [("authorization", auth.as_str())];
    let p = |r: &str, kind: &str, key: &str, parent: Option<&str>, payload: serde_json::Value| json!({"ref": r, "kind": kind, "tempKey": key, "parentTempKey": parent, "payload": payload});
    let mc = |two: bool| json!({"prompt": "Var(2X)?", "keyReliability": "Official", "options": [{"text": "4 Var(X)", "isCorrect": true}, {"text": "2 Var(X)", "isCorrect": two}]});
    // Sent out of order on purpose: core sorts topologically.
    let proposals = json!([
        p(
            "01PROPOSAL0000000000000010",
            "Question",
            "q1",
            Some("l1"),
            json!({"type": "MC", "content": mc(false), "paperTempKey": "p1", "paperQuestionNo": "3", "conceptTempKeys": ["c1"]})
        ),
        p("01PROPOSAL0000000000000011", "Question", "q2", Some("l1"), json!({"type": "MC", "content": mc(true)})),
        p("01PROPOSAL0000000000000012", "Question", "q3", Some("l9"), json!({"type": "MC", "content": mc(false)})),
        p("01PROPOSAL0000000000000001", "Course", "course", None, json!({"name": "Imported stats", "code": "IMP"})),
        p("01PROPOSAL0000000000000002", "Component", "l1", Some("course"), json!({"code": "L1", "name": "Variance", "estHours": 2})),
        p("01PROPOSAL0000000000000003", "Concept", "c1", None, json!({"name": "Variance rules"})),
        p(
            "01PROPOSAL0000000000000004",
            "Material",
            "m1",
            Some("l1"),
            json!({"kind": "Notes", "title": "Summary", "content": "Var(aX)=a^2Var(X)"})
        ),
        p("01PROPOSAL0000000000000005", "Paper", "p1", None, json!({"paperKey": "2025-A", "label": "Exam 2025"})),
        p("01PROPOSAL0000000000000006", "Flashcard", "f1", Some("l1"), json!({"front": "Var(aX)?", "back": "a^2 Var(X)"})),
        p(
            "01PROPOSAL0000000000000007",
            "Deadline",
            "d1",
            None,
            json!({"type": "Exam", "title": "Final", "startsAt": "2026-12-01T09:00:00Z", "durationMin": 120})
        ),
        p("01PROPOSAL0000000000000008", "DeadlineComponent", "dc1", Some("d1"), json!({"componentTempKey": "l1", "questionCount": 10})),
        p("01PROPOSAL0000000000000009", "ExamAids", "aids", None, json!({"calculator": "Simple calculator"}))
    ]);
    let body = json!({"user": me, "importRef": "01IMPORT000000000000000001", "proposals": proposals});
    let r = call(&h.app, "POST", "/internal/v1/import/apply", None, Some(body.clone()), &headers).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    let failed: Vec<(&str, &str)> =
        r.json["failed"].as_array().unwrap().iter().map(|f| (f["proposalRef"].as_str().unwrap(), f["reason"].as_str().unwrap())).collect();
    assert_eq!(failed.len(), 2, "{failed:?}");
    assert!(failed.iter().any(|(p, reason)| p.ends_with("11") && reason.contains("options")), "{failed:?}");
    assert!(failed.iter().any(|(p, reason)| p.ends_with("12") && *reason == "parent_component"), "{failed:?}");
    let course = r.json["courseRef"].as_str().unwrap().to_string();
    let q1 = r.json["created"].as_array().unwrap().iter().find(|c| c["proposalRef"] == "01PROPOSAL0000000000000010").unwrap()["entityRef"]
        .as_str()
        .unwrap()
        .to_string();
    let q = call(&h.app, "GET", &format!("/questions/{q1}"), Some(&t), None, &[]).await.json;
    assert_eq!(
        (q["source"].as_str(), q["current"]["keyReliability"].as_str(), q["paperQuestionNo"].as_str()),
        (Some("PastExam"), Some("Official"), Some("3")),
        "{q}"
    );
    let cards = call(&h.app, "GET", &format!("/courses/{course}/flashcards"), Some(&t), None, &[]).await.json;
    assert_eq!(cards[0]["code"], "IMP001");
    let deadlines = call(&h.app, "GET", &format!("/courses/{course}/deadlines"), Some(&t), None, &[]).await.json;
    assert_eq!(deadlines[0]["layout"][0]["questionCount"], 10);

    // Idempotent on the import ref.
    let again = call(&h.app, "POST", "/internal/v1/import/apply", None, Some(body), &headers).await;
    assert_eq!(again.json, r.json);
    let courses = call(&h.app, "GET", "/courses", Some(&t), None, &[]).await.json;
    assert_eq!(courses.as_array().unwrap().len(), 1, "applied once");

    // The catalog the duplicate check compares against; an accepted duplicate lecture links to the
    // existing one, so its children land there and no second L1 appears.
    let cat = call(&h.app, "GET", &format!("/internal/v1/ai/import-catalog?user={me}&course={course}"), None, None, &headers).await.json;
    let l1 = cat["components"][0]["id"].as_str().unwrap().to_string();
    assert_eq!((cat["components"][0]["code"].as_str(), cat["questions"].as_array().unwrap().len()), (Some("L1"), 1), "{cat}");
    assert_eq!(cat["flashcards"][0]["front"], "Var(aX)?");
    let mut dup = p("01PROPOSAL0000000000000020", "Component", "k1", None, json!({"code": "L1", "name": "Variance (again)"}));
    dup["duplicateOf"] = json!(l1);
    let more = json!({"user": me, "importRef": "01IMPORT000000000000000003", "course": course, "proposals": [
        dup,
        p("01PROPOSAL0000000000000021", "Flashcard", "f2", Some("k1"), json!({"front": "Var(X+c)?", "back": "Var(X)"}))
    ]});
    let r = call(&h.app, "POST", "/internal/v1/import/apply", None, Some(more), &headers).await;
    assert_eq!(r.json["created"][0]["entityRef"].as_str(), Some(l1.as_str()), "{}", r.json);
    let comps = call(&h.app, "GET", &format!("/courses/{course}/components"), Some(&t), None, &[]).await.json;
    assert_eq!(comps.as_array().unwrap().len(), 1, "no second L1: {comps}");
    let cards = call(&h.app, "GET", &format!("/courses/{course}/flashcards"), Some(&t), None, &[]).await.json;
    assert!(cards.as_array().unwrap().iter().all(|c| c["componentId"] == l1.as_str()), "{cards}");
    let r = call(&h.app, "GET", &format!("/internal/v1/ai/import-catalog?user={bob}&course={course}"), None, None, &headers).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);

    // Into someone else's course: 404. The paper lookup needs edit rights too.
    let other = json!({"user": bob, "importRef": "01IMPORT000000000000000002", "course": course, "proposals": []});
    let r = call(&h.app, "POST", "/internal/v1/import/apply", None, Some(other), &headers).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let paper =
        call(&h.app, "GET", &format!("/courses/{course}/papers"), Some(&t), None, &[]).await.json[0]["id"].as_str().unwrap().to_string();
    let r = call(&h.app, "GET", &format!("/internal/v1/ai/paper?user={me}&paper={paper}"), None, None, &headers).await;
    assert_eq!(r.json["courseId"].as_str(), Some(course.as_str()));
    let r = call(&h.app, "GET", &format!("/internal/v1/ai/paper?user={bob}&paper={paper}"), None, None, &headers).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);

    // A paper proposal (or the user's edit of one) naming someone else's file fails, like the
    // public papers route would (audit H3): a paper grants its readers the file.
    h.t.files.put(crux_clients::files::FileInfo {
        id: "01FILEOFBOB000000000000000".into(),
        owner_ref: bob.clone(),
        uploaded_by_ref: Some(bob.clone()),
        course_ref: None,
        purpose: "Original".into(),
        status: "Active".into(),
        scan_status: "Clean".into(),
        extract_status: "Done".into(),
        sha256: "0".repeat(64),
        size_bytes: 10,
        page_count: Some(1),
        file_name: "private.pdf".into(),
    });
    let steal = json!({"user": me, "importRef": "01IMPORT000000000000000004", "course": course, "proposals": [
        p("01PROPOSAL0000000000000030", "Paper", "p9", None, json!({"paperKey": "2024-X", "fileRef": "01FILEOFBOB000000000000000"}))
    ]});
    let r = call(&h.app, "POST", "/internal/v1/import/apply", None, Some(steal), &headers).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["created"].as_array().map(Vec::len), Some(0), "{}", r.json);
    assert_eq!(r.json["failed"][0]["proposalRef"], "01PROPOSAL0000000000000030", "{}", r.json);
    let _ = bt;
    h.db.drop_db().await;
}
