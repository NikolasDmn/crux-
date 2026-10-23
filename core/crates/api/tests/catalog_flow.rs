//! Milestone 2 core: courses, sharing (D2-1..D2-3), lectures, materials, papers, trash (D2-5).

mod common;

use axum::http::StatusCode;
use crux_clients::files::FileInfo;
use crux_testkit::{call, outbox_topics, signup, verify_email, Resp};
use serde_json::{json, Value};

async fn course(h: &common::Harness, token: &str, code: &str) -> Value {
    let r = call(&h.app, "POST", "/courses", Some(token), Some(json!({"name": format!("Course {code}"), "code": code})), &[]).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    r.json
}

fn ifm(v: &Value) -> String {
    v["version"].as_i64().unwrap().to_string()
}

async fn outbox_payloads(h: &common::Harness, topic: &str) -> Vec<Value> {
    let rows: Vec<(sqlx::types::Json<Value>,)> =
        sqlx::query_as("SELECT Payload FROM Outbox WHERE Topic = ? ORDER BY Id").bind(topic).fetch_all(&h.db.pool).await.unwrap();
    rows.into_iter().map(|r| r.0 .0).collect()
}

#[tokio::test]
async fn course_defaults_and_edit_rules() {
    let Some(h) = common::harness().await else { return };
    let (t, _, _) = signup(&h.app, &h.t.ctx, "owner").await;
    let c1 = course(&h, &t, "bsa").await;
    assert_eq!(c1["code"], "BSA", "codes are upper-cased");
    assert_eq!(c1["gradeScale"]["name"], "Percent", "D2-6");
    assert_eq!(c1["myRole"], "Owner");
    assert_eq!(c1["mySubscription"], "Active", "the owner is subscribed");
    let c2 = course(&h, &t, "RK").await;
    assert_ne!(c1["color"], c2["color"], "round-robin palette");

    let dup = call(&h.app, "POST", "/courses", Some(&t), Some(json!({"name": "x", "code": "BSA"})), &[]).await;
    assert_eq!(dup.json["error"]["details"]["fields"][0]["code"], "taken");

    let id = c1["id"].as_str().unwrap();
    let scales = call(&h.app, "GET", "/grade-scales", Some(&t), None, &[]).await;
    let nl = scales.json.as_array().unwrap().iter().find(|s| s["name"] == "NL 1-10").unwrap()["id"].clone();
    let p = call(
        &h.app,
        "PATCH",
        &format!("/courses/{id}"),
        Some(&t),
        Some(json!({"color": "#123456", "gradeScaleId": nl, "term": "2026 Q1"})),
        &[("if-match", &ifm(&c1))],
    )
    .await;
    assert_eq!(p.status, StatusCode::OK, "{}", p.json);
    assert_eq!(
        (p.json["color"].as_str(), p.json["gradeScale"]["name"].as_str(), p.json["term"].as_str()),
        (Some("#123456"), Some("NL 1-10"), Some("2026 Q1"))
    );
    assert!(outbox_topics(&h.db.pool).await.contains(&"course.updated".to_string()));
    h.db.drop_db().await;
}

#[tokio::test]
async fn sharing_needs_acceptance_and_roles_hold() {
    let Some(h) = common::harness().await else { return };
    let (owner, _, _) = signup(&h.app, &h.t.ctx, "owner").await;
    let (viewer, _, _) = signup(&h.app, &h.t.ctx, "viewer").await;
    let (editor, _, _) = signup(&h.app, &h.t.ctx, "editor").await;
    let c = course(&h, &owner, "BSA").await;
    let id = c["id"].as_str().unwrap();

    let shared = call(
        &h.app,
        "POST",
        &format!("/courses/{id}/members"),
        Some(&owner),
        Some(json!({"usernameOrEmail": "viewer", "role": "Viewer"})),
        &[],
    )
    .await;
    assert_eq!(shared.status, StatusCode::OK, "{}", shared.json);
    assert_eq!(shared.json["members"][0]["status"], "Pending");
    call(
        &h.app,
        "POST",
        &format!("/courses/{id}/members"),
        Some(&owner),
        Some(json!({"usernameOrEmail": "editor", "role": "Editor"})),
        &[],
    )
    .await;

    // D2-2: nothing before accepting.
    assert_eq!(call(&h.app, "GET", &format!("/courses/{id}"), Some(&viewer), None, &[]).await.status, StatusCode::NOT_FOUND);
    assert_eq!(call(&h.app, "GET", "/courses", Some(&viewer), None, &[]).await.json.as_array().unwrap().len(), 0);
    let inv = call(&h.app, "GET", "/course-invitations", Some(&viewer), None, &[]).await;
    assert_eq!(inv.json[0]["courseId"], id);
    for t in [&viewer, &editor] {
        let a = call(&h.app, "POST", &format!("/course-invitations/{id}/accept"), Some(t), None, &[]).await;
        assert_eq!(a.status, StatusCode::OK, "{}", a.json);
    }
    let seen = call(&h.app, "GET", &format!("/courses/{id}"), Some(&viewer), None, &[]).await;
    assert_eq!((seen.status, seen.json["myRole"].as_str()), (StatusCode::OK, Some("Viewer")));
    assert_eq!(seen.json["mySubscription"], Value::Null, "subscribing is a separate step");

    // Viewer reads, can't write; editor writes but can't do owner-only things.
    let lecture = json!({"code": "L1", "name": "Intro"});
    assert_eq!(
        call(&h.app, "POST", &format!("/courses/{id}/components"), Some(&viewer), Some(lecture.clone()), &[]).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&h.app, "POST", &format!("/courses/{id}/components"), Some(&editor), Some(lecture), &[]).await.status,
        StatusCode::CREATED
    );
    assert_eq!(call(&h.app, "GET", &format!("/courses/{id}/components"), Some(&viewer), None, &[]).await.json.as_array().unwrap().len(), 1);
    let colour =
        call(&h.app, "PATCH", &format!("/courses/{id}"), Some(&editor), Some(json!({"color": "#000000"})), &[("if-match", &ifm(&c))]).await;
    assert_eq!(colour.status, StatusCode::NOT_FOUND, "colour is owner-only (spec 4.2)");
    let rename =
        call(&h.app, "PATCH", &format!("/courses/{id}"), Some(&editor), Some(json!({"name": "Renamed"})), &[("if-match", &ifm(&c))]).await;
    assert_eq!(rename.status, StatusCode::OK, "{}", rename.json);
    assert_eq!(call(&h.app, "GET", &format!("/courses/{id}/members"), Some(&editor), None, &[]).await.status, StatusCode::NOT_FOUND);

    // Members of the course get the change events; the viewer leaves on their own.
    let changes = outbox_payloads(&h, "course.member_changed").await;
    assert_eq!(changes.len(), 2);
    let me = call(&h.app, "GET", "/me", Some(&viewer), None, &[]).await;
    let left =
        call(&h.app, "DELETE", &format!("/courses/{id}/members/{}", me.json["id"].as_str().unwrap()), Some(&viewer), None, &[]).await;
    assert_eq!(left.status, StatusCode::NO_CONTENT);
    assert_eq!(call(&h.app, "GET", &format!("/courses/{id}"), Some(&viewer), None, &[]).await.status, StatusCode::NOT_FOUND);
    h.db.drop_db().await;
}

#[tokio::test]
async fn sharing_with_a_new_email_invites_and_signup_accepts() {
    let Some(h) = common::harness().await else { return };
    let (owner, user, _) = signup(&h.app, &h.t.ctx, "owner").await;
    let c = course(&h, &owner, "BSA").await;
    let id = c["id"].as_str().unwrap();
    let body = json!({"usernameOrEmail": "Sam@Uni.nl", "role": "Editor"});

    let unverified = call(&h.app, "POST", &format!("/courses/{id}/members"), Some(&owner), Some(body.clone()), &[]).await;
    assert_eq!(unverified.json["error"]["code"], "email_unverified", "invites need a verified email (spec 4.1)");
    verify_email(&h.t.ctx, user["id"].as_str().unwrap()).await;
    let r = call(&h.app, "POST", &format!("/courses/{id}/members"), Some(&owner), Some(body.clone()), &[]).await;
    assert_eq!(r.json["pendingEmails"][0]["email"], "sam@uni.nl");
    call(&h.app, "POST", &format!("/courses/{id}/members"), Some(&owner), Some(body), &[]).await;
    let mails: Vec<Value> =
        outbox_payloads(&h, "notify.requested").await.into_iter().filter(|p| p["kind"] == "invite.course_shared").collect();
    assert_eq!(mails.len(), 1, "sharing twice doesn't send twice");
    let link = mails[0]["payload"]["link"].as_str().unwrap();
    let code = link.split("invite=").nth(1).unwrap();

    let joined = call(
        &h.app,
        "POST",
        "/auth/signup",
        None,
        Some(json!({
            "inviteCode": code, "email": "sam@uni.nl", "username": "sam", "firstName": "Sam", "lastName": "S",
            "password": "correct horse battery", "timeZone": "Europe/Amsterdam"
        })),
        &[],
    )
    .await;
    assert_eq!(joined.status, StatusCode::OK, "{}", joined.json);
    let sam = joined.json["accessToken"].as_str().unwrap();
    let seen = call(&h.app, "GET", &format!("/courses/{id}"), Some(sam), None, &[]).await;
    assert_eq!((seen.status, seen.json["myRole"].as_str()), (StatusCode::OK, Some("Editor")), "D2-3: the link counts as accepting");
    let members = call(&h.app, "GET", &format!("/courses/{id}/members"), Some(&owner), None, &[]).await;
    assert_eq!(members.json["pendingEmails"].as_array().unwrap().len(), 0);
    assert_eq!(members.json["members"][0]["status"], "Accepted");
    h.db.drop_db().await;
}

#[tokio::test]
async fn plain_invite_with_email_is_emailed() {
    let Some(h) = common::harness().await else { return };
    let (t, u, _) = signup(&h.app, &h.t.ctx, "inviter").await;
    verify_email(&h.t.ctx, u["id"].as_str().unwrap()).await;
    call(&h.app, "POST", "/invites", Some(&t), Some(json!({"email": "friend@uni.nl"})), &[]).await;
    call(&h.app, "POST", "/invites", Some(&t), Some(json!({})), &[]).await;
    let mails: Vec<Value> = outbox_payloads(&h, "notify.requested").await.into_iter().filter(|p| p["kind"] == "invite.created").collect();
    assert_eq!(mails.len(), 1, "D2-9: only the invite with an address is emailed");
    assert_eq!(mails[0]["payload"]["to"], "friend@uni.nl");
    h.db.drop_db().await;
}

#[tokio::test]
async fn lectures_prerequisites_and_cycles() {
    let Some(h) = common::harness().await else { return };
    let (t, _, _) = signup(&h.app, &h.t.ctx, "owner").await;
    let c = course(&h, &t, "BSA").await;
    let id = c["id"].as_str().unwrap();
    let mk = |code: &str, pre: Vec<Value>| json!({"code": code, "name": code, "estHours": 2.5, "prerequisiteIds": pre});
    let l1 = call(&h.app, "POST", &format!("/courses/{id}/components"), Some(&t), Some(mk("L1", vec![])), &[]).await.json;
    let l2 = call(&h.app, "POST", &format!("/courses/{id}/components"), Some(&t), Some(mk("L2", vec![l1["id"].clone()])), &[]).await.json;
    assert_eq!(l2["prerequisiteIds"][0], l1["id"]);
    assert!(l2["sortOrder"].as_i64() > l1["sortOrder"].as_i64());

    let cycle = call(
        &h.app,
        "PATCH",
        &format!("/components/{}", l1["id"].as_str().unwrap()),
        Some(&t),
        Some(json!({"prerequisiteIds": [l2["id"]]})),
        &[("if-match", &ifm(&l1))],
    )
    .await;
    assert_eq!(cycle.json["error"]["details"]["fields"][0]["code"], "cycle");

    let other = course(&h, &t, "RK").await;
    let foreign = call(
        &h.app,
        "POST",
        &format!("/courses/{}/components", other["id"].as_str().unwrap()),
        Some(&t),
        Some(mk("X1", vec![l1["id"].clone()])),
        &[],
    )
    .await;
    assert_eq!(foreign.json["error"]["details"]["fields"][0]["code"], "unknown", "same course only");

    let dup = call(&h.app, "POST", &format!("/courses/{id}/components"), Some(&t), Some(mk("L1", vec![])), &[]).await;
    assert_eq!(dup.json["error"]["details"]["fields"][0]["code"], "taken");
    let gone = call(&h.app, "DELETE", &format!("/components/{}", l2["id"].as_str().unwrap()), Some(&t), None, &[]).await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT);
    assert_eq!(call(&h.app, "GET", &format!("/courses/{id}/components"), Some(&t), None, &[]).await.json.as_array().unwrap().len(), 1);
    h.db.drop_db().await;
}

fn file(id: &str, owner: &str, course: Option<&str>, scan: &str) -> FileInfo {
    FileInfo {
        id: id.into(),
        owner_ref: owner.into(),
        uploaded_by_ref: Some(owner.into()),
        course_ref: course.map(String::from),
        purpose: "Original".into(),
        status: "Active".into(),
        scan_status: scan.into(),
        extract_status: "Pending".into(),
        sha256: "a".repeat(64),
        size_bytes: 1000,
        page_count: None,
        file_name: "slides.pdf".into(),
    }
}

#[tokio::test]
async fn materials_need_clean_files_of_the_course() {
    let Some(h) = common::harness().await else { return };
    let (t, u, _) = signup(&h.app, &h.t.ctx, "owner").await;
    let me = u["id"].as_str().unwrap();
    let c = course(&h, &t, "BSA").await;
    let id = c["id"].as_str().unwrap();
    let l1 =
        call(&h.app, "POST", &format!("/courses/{id}/components"), Some(&t), Some(json!({"code": "L1", "name": "Intro"})), &[]).await.json;
    let mats = format!("/components/{}/materials", l1["id"].as_str().unwrap());

    let md = call(&h.app, "POST", &mats, Some(&t), Some(json!({"kind": "Notes", "title": "Week 1", "content": "# Hi $x^2$"})), &[]).await;
    assert_eq!((md.status, md.json["originalStatus"].as_str()), (StatusCode::CREATED, Some("None")), "{}", md.json);

    h.t.files.put(file("01FILECLEAN000000000000000", me, Some(id), "Clean"));
    h.t.files.put(file("01FILEPENDING0000000000000", me, Some(id), "Pending"));
    h.t.files.put(file("01FILESTRANGER000000000000", "01SOMEONEELSE0000000000000", None, "Clean"));
    let ok = call(
        &h.app,
        "POST",
        &mats,
        Some(&t),
        Some(json!({"kind": "Slides", "title": "Slides", "fileId": "01FILECLEAN000000000000000"})),
        &[],
    )
    .await;
    assert_eq!((ok.status, ok.json["originalStatus"].as_str()), (StatusCode::CREATED, Some("Present")), "{}", ok.json);
    let pending =
        call(&h.app, "POST", &mats, Some(&t), Some(json!({"kind": "Slides", "title": "x", "fileId": "01FILEPENDING0000000000000"})), &[])
            .await;
    assert_eq!(pending.json["error"]["details"]["fields"][0]["code"], "not_ready");
    let theirs =
        call(&h.app, "POST", &mats, Some(&t), Some(json!({"kind": "Slides", "title": "x", "fileId": "01FILESTRANGER000000000000"})), &[])
            .await;
    assert_eq!(theirs.json["error"]["details"]["fields"][0]["code"], "unknown", "someone else's file");

    let p = call(
        &h.app,
        "PATCH",
        &format!("/materials/{}", ok.json["id"].as_str().unwrap()),
        Some(&t),
        Some(json!({"excluded": true, "aiAllowed": false})),
        &[("if-match", &ifm(&ok.json))],
    )
    .await;
    assert_eq!((p.json["excluded"].as_bool(), p.json["aiAllowed"].as_bool()), (Some(true), Some(false)), "{}", p.json);
    let events = outbox_payloads(&h, "material.updated").await;
    assert_eq!(events.last().unwrap()["aiAllowed"], false);

    // Access to a file through a course (spec 6.1 step 7): owner yes, stranger no.
    let (other, ou, _) = signup(&h.app, &h.t.ctx, "stranger").await;
    let svc = h.t.ctx.keys.sign_service("files", "core", h.t.ctx.clock.now().timestamp());
    let ask = |user: &str| format!("/internal/v1/access/file-ref?user={user}&ref=01FILECLEAN000000000000000");
    let yes = call(&h.app, "GET", &ask(me), Some(&svc), None, &[]).await;
    let no = call(&h.app, "GET", &ask(ou["id"].as_str().unwrap()), Some(&svc), None, &[]).await;
    assert_eq!((yes.json["allowed"].as_bool(), no.json["allowed"].as_bool()), (Some(true), Some(false)));
    let role =
        call(&h.app, "GET", &format!("/internal/v1/access?user={}&course={id}", ou["id"].as_str().unwrap()), Some(&svc), None, &[]).await;
    assert_eq!((role.json["role"].clone(), role.json["ownerRef"].as_str()), (Value::Null, Some(me)));
    let user_token = call(&h.app, "GET", &ask(me), Some(&other), None, &[]).await;
    assert_eq!(user_token.status, StatusCode::UNAUTHORIZED, "internal routes need a service token");
    h.db.drop_db().await;
}

#[tokio::test]
async fn papers_keep_their_exam_map() {
    let Some(h) = common::harness().await else { return };
    let (t, _, _) = signup(&h.app, &h.t.ctx, "owner").await;
    let id = course(&h, &t, "BSA").await["id"].as_str().unwrap().to_string();
    let p = call(
        &h.app,
        "POST",
        &format!("/courses/{id}/papers"),
        Some(&t),
        Some(json!({
            "paperKey": "2024-06", "label": "June 2024", "examDate": "2024-06-20", "totalQuestions": 40, "keyStatus": "Official",
            "omissions": [{"questionNumber": "12", "reason": "Not in this year's syllabus"}]
        })),
        &[],
    )
    .await;
    assert_eq!(p.status, StatusCode::CREATED, "{}", p.json);
    assert_eq!(p.json["omissions"][0]["questionNumber"], "12");
    let upd = call(
        &h.app,
        "PATCH",
        &format!("/papers/{}", p.json["id"].as_str().unwrap()),
        Some(&t),
        Some(json!({"omissions": []})),
        &[("if-match", &ifm(&p.json))],
    )
    .await;
    assert_eq!(upd.json["omissions"].as_array().unwrap().len(), 0, "{}", upd.json);
    let stale = call(
        &h.app,
        "PATCH",
        &format!("/papers/{}", p.json["id"].as_str().unwrap()),
        Some(&t),
        Some(json!({"label": "x"})),
        &[("if-match", &ifm(&p.json))],
    )
    .await;
    assert_eq!(stale.status, StatusCode::CONFLICT);
    let list: Resp = call(&h.app, "GET", &format!("/courses/{id}/papers"), Some(&t), None, &[]).await;
    assert_eq!(list.json[0]["examDate"], "2024-06-20");
    h.db.drop_db().await;
}

#[tokio::test]
async fn trash_restore_and_purge() {
    let Some(h) = common::harness().await else { return };
    let (owner, _, _) = signup(&h.app, &h.t.ctx, "owner").await;
    let (member, _, _) = signup(&h.app, &h.t.ctx, "member").await;
    let id = course(&h, &owner, "BSA").await["id"].as_str().unwrap().to_string();
    call(
        &h.app,
        "POST",
        &format!("/courses/{id}/members"),
        Some(&owner),
        Some(json!({"usernameOrEmail": "member", "role": "Viewer"})),
        &[],
    )
    .await;
    call(&h.app, "POST", &format!("/course-invitations/{id}/accept"), Some(&member), None, &[]).await;

    let trashed = call(&h.app, "DELETE", &format!("/courses/{id}"), Some(&owner), None, &[]).await;
    assert!(trashed.json["purgeAt"].is_string(), "{}", trashed.json);
    assert_eq!(
        call(&h.app, "GET", &format!("/courses/{id}"), Some(&member), None, &[]).await.status,
        StatusCode::NOT_FOUND,
        "hidden from everyone"
    );
    assert_eq!(call(&h.app, "GET", &format!("/courses/{id}"), Some(&owner), None, &[]).await.status, StatusCode::NOT_FOUND);
    assert_eq!(call(&h.app, "GET", "/courses?trashed=true", Some(&owner), None, &[]).await.json[0]["id"], id.as_str());
    assert_eq!(call(&h.app, "POST", &format!("/courses/{id}/restore"), Some(&member), None, &[]).await.status, StatusCode::NOT_FOUND);
    assert_eq!(call(&h.app, "POST", &format!("/courses/{id}/restore"), Some(&owner), None, &[]).await.status, StatusCode::OK);
    assert_eq!(call(&h.app, "GET", &format!("/courses/{id}"), Some(&member), None, &[]).await.status, StatusCode::OK);

    call(&h.app, "DELETE", &format!("/courses/{id}"), Some(&owner), None, &[]).await;
    h.t.clock.advance(chrono::Duration::days(29));
    assert_eq!(crux_service::courses::purge_trashed(&h.t.ctx).await.unwrap(), 0);
    h.t.clock.advance(chrono::Duration::days(1));
    assert_eq!(crux_service::courses::purge_trashed(&h.t.ctx).await.unwrap(), 1);
    // (The access token expired during the time travel; check the rows directly.)
    let left: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM Course").fetch_one(&h.db.pool).await.unwrap();
    assert_eq!(left.0, 0, "purged for good");
    assert_eq!(outbox_payloads(&h, "course.deleted").await[0]["courseRef"], id.as_str());
    h.db.drop_db().await;
}
