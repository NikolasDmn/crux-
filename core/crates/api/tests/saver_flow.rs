//! Milestone 7: storage saver on core's side (spec 6.2): free space, undo, per course, and the
//! materials following files' purge / restore events.

mod common;

use axum::http::StatusCode;
use crux_clients::bus::{Delivery, Envelope};
use crux_clients::files::FileInfo;
use crux_jobs::consumer::{process, Outcome};
use crux_testkit::{call, outbox_topics, signup};
use serde_json::json;

fn file(id: &str, owner: &str, course: &str) -> FileInfo {
    FileInfo {
        id: id.into(),
        owner_ref: owner.into(),
        uploaded_by_ref: Some(owner.into()),
        course_ref: Some(course.into()),
        purpose: "Original".into(),
        status: "Active".into(),
        scan_status: "Clean".into(),
        extract_status: "Done".into(),
        sha256: "b".repeat(64),
        size_bytes: 1000,
        page_count: Some(3),
        file_name: "slides.pdf".into(),
    }
}

fn delivery(n: u32, topic: &str, payload: serde_json::Value) -> Delivery {
    Delivery {
        stream_id: format!("{n}-0"),
        topic: topic.into(),
        envelope: Envelope {
            event_id: format!("01SAVEREVENT{n:014}"),
            topic: topic.into(),
            schema_version: 1,
            occurred_at: "2026-10-23T12:00:00.000Z".into(),
            producer: "files".into(),
            aggregate_ref: None,
            payload,
        },
    }
}

#[tokio::test]
async fn free_space_undo_and_purge_events() {
    let Some(h) = common::harness().await else { return };
    let (t, me, _) = signup(&h.app, &h.t.ctx, "olga").await;
    let me = me["id"].as_str().unwrap().to_string();
    let (vera, _, _) = signup(&h.app, &h.t.ctx, "vera").await;
    let course = call(&h.app, "POST", "/courses", Some(&t), Some(json!({"name": "Stats", "code": "ST"})), &[]).await.json["id"]
        .as_str()
        .unwrap()
        .to_string();
    let l1 = call(&h.app, "POST", &format!("/courses/{course}/components"), Some(&t), Some(json!({"code": "L1", "name": "L1"})), &[])
        .await
        .json["id"]
        .as_str()
        .unwrap()
        .to_string();
    let fid = "01FILESAVER000000000000001";
    h.t.files.put(file(fid, &me, &course));
    let mats = format!("/components/{l1}/materials");
    let m = call(&h.app, "POST", &mats, Some(&t), Some(json!({"kind": "Slides", "title": "Week 1", "fileId": fid})), &[]).await;
    assert_eq!(m.status, StatusCode::CREATED, "{}", m.json);
    let mid = m.json["id"].as_str().unwrap().to_string();
    let notes = call(&h.app, "POST", &mats, Some(&t), Some(json!({"kind": "Notes", "title": "N", "content": "x"})), &[]).await.json["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Only the owner frees space; a material without a file has nothing to free.
    call(&h.app, "POST", &format!("/courses/{course}/members"), Some(&t), Some(json!({"usernameOrEmail": "vera", "role": "Editor"})), &[])
        .await;
    call(&h.app, "POST", &format!("/course-invitations/{course}/accept"), Some(&vera), None, &[]).await;
    let r = call(&h.app, "POST", &format!("/materials/{mid}/free-space"), Some(&vera), None, &[]).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND, "an Editor can't");
    let r = call(&h.app, "POST", &format!("/materials/{notes}/free-space"), Some(&t), None, &[]).await;
    assert_eq!(r.json["error"]["details"]["fields"][0]["code"], "no_original");

    let r = call(&h.app, "POST", &format!("/materials/{mid}/free-space"), Some(&t), None, &[]).await;
    assert_eq!((r.status, r.json["originalStatus"].as_str()), (StatusCode::OK, Some("PendingPurge")), "{}", r.json);
    assert_eq!(r.json["originalPurgeAt"], "2026-10-30T12:00:00.000Z");
    assert!(outbox_topics(&h.db.pool).await.contains(&"material.purge_requested".to_string()));
    let r = call(&h.app, "POST", &format!("/materials/{mid}/free-space/cancel"), Some(&t), None, &[]).await;
    assert_eq!(r.json["originalStatus"], "Present", "{}", r.json);
    let r = call(&h.app, "POST", &format!("/materials/{mid}/free-space/cancel"), Some(&t), None, &[]).await;
    assert_eq!(r.status, StatusCode::CONFLICT, "nothing to undo");

    let r = call(&h.app, "POST", &format!("/courses/{course}/free-space"), Some(&t), None, &[]).await;
    assert_eq!(r.json["files"], 1, "{}", r.json);

    // Files reports the purge, then a re-upload restoring it.
    let handler = crux_jobs::files::FileEvents;
    let purged = json!({"fileRef": fid, "ownerRef": me, "sha256": "b".repeat(64), "purgedAt": "2026-10-30T12:00:00.000Z"});
    assert_eq!(process(&h.db.pool, &handler, &delivery(1, "file.purged", purged.clone())).await.unwrap(), Outcome::Applied);
    assert_eq!(process(&h.db.pool, &handler, &delivery(1, "file.purged", purged)).await.unwrap(), Outcome::Duplicate);
    let list = call(&h.app, "GET", &mats, Some(&t), None, &[]).await.json;
    let m = list.as_array().unwrap().iter().find(|x| x["id"] == mid.as_str()).unwrap().clone();
    assert_eq!((m["originalStatus"].as_str(), m["originalPurgeAt"].is_null()), (Some("Purged"), true));
    let restored = json!({"fileRef": fid, "ownerRef": me, "sha256": "b".repeat(64)});
    process(&h.db.pool, &handler, &delivery(2, "file.restored", restored)).await.unwrap();
    let list = call(&h.app, "GET", &mats, Some(&t), None, &[]).await.json;
    assert!(list.as_array().unwrap().iter().any(|x| x["id"] == mid.as_str() && x["originalStatus"] == "Present"));

    // An automatic purge scheduled by files shows as pending; a bad payload is refused.
    let sched = json!({"fileRef": fid, "ownerRef": me, "purgeAt": "2026-11-01T00:00:00.000Z", "reason": "auto"});
    process(&h.db.pool, &handler, &delivery(3, "file.purge_scheduled", sched)).await.unwrap();
    let list = call(&h.app, "GET", &mats, Some(&t), None, &[]).await.json;
    assert!(list.as_array().unwrap().iter().any(|x| x["id"] == mid.as_str() && x["originalPurgeAt"] == "2026-11-01T00:00:00.000Z"));
    assert!(process(&h.db.pool, &handler, &delivery(4, "file.purged", json!({"fileRef": fid}))).await.is_err());
    h.db.drop_db().await;
}
