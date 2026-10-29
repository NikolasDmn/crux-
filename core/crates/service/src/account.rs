//! Account export and deletion (backend-spec 11, 9.4).
//!
//! Export: `POST /account/export` queues it; the job dumps the user's core rows, asks ai and notify
//! for their parts, and files zips everything with the originals. The user is notified with a
//! download valid for 24 h.
//!
//! Deletion: `POST /account/delete` (password + what to do with owned courses others use) starts a
//! 14-day grace period, cancellable. Then the job transfers or deletes those courses, emits
//! `user.deleted`, and once files, ai and notify have acked with `user.purged`, deletes the user.

use chrono::{Duration, Utc};
use crux_domain::topics;
use crux_domain::ValidationErrors;
use crux_repo::account as repo;
use crux_repo::MySqlConnection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::ToSchema;

use crate::time::{iso, iso_opt};
use crate::{Actor, AppError, AppResult, Ctx};

pub const GRACE_DAYS: i64 = 14;
pub const EXPORT_HOURS: i64 = 24;
/// Services that must ack `user.deleted` before the Users row goes (spec 9.4).
pub const ACK_SERVICES: [&str; 3] = ["files", "ai", "notify"];
/// Audit M3: a service that isn't deployed (ai is optional) never acks. After this long the
/// deletion finishes anyway, with an alert naming the services that didn't answer.
pub const ACK_TIMEOUT_DAYS: i64 = 3;

// ---------- export ----------

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportView {
    pub id: String,
    /// Queued, Building, Ready, Failed
    pub status: String,
    pub requested_at: String,
    pub ready_at: Option<String>,
    /// The download (GET /files/{fileId}/url) works until then.
    pub expires_at: Option<String>,
    pub file_id: Option<String>,
    pub size_bytes: Option<i64>,
}

fn export_view(r: &repo::ExportRow, now: chrono::DateTime<Utc>) -> ExportView {
    let expired = r.expires_at.is_some_and(|e| e <= now);
    ExportView {
        id: r.public_id.clone(),
        status: if expired { "Expired".into() } else { r.status.clone() },
        requested_at: iso(r.requested_at),
        ready_at: iso_opt(r.ready_at),
        expires_at: iso_opt(r.expires_at),
        file_id: if expired { None } else { r.file_ref.clone() },
        size_bytes: r.size_bytes,
    }
}

pub async fn request_export(ctx: &Ctx, actor: &Actor) -> AppResult<ExportView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let now = ctx.clock.now();
    if repo::open_export(&mut tx, actor.user_id).await?.is_none() {
        repo::insert_export(&mut tx, &ctx.ids.ulid(), actor.user_id, now).await?;
    }
    let row = repo::exports(&mut tx, actor.user_id).await?.into_iter().next().ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(export_view(&row, now))
}

pub async fn list_exports(ctx: &Ctx, actor: &Actor) -> AppResult<Vec<ExportView>> {
    let mut conn = ctx.pool.acquire().await?;
    let now = ctx.clock.now();
    Ok(repo::exports(&mut conn, actor.user_id).await?.iter().map(|r| export_view(r, now)).collect())
}

/// Core tables that hold nothing of a user's (audit M4: every other table is exported; a test
/// checks that each table is one or the other).
pub const NOT_PERSONAL: &[&str] = &["ConsumedEvent", "Outbox", "Tier"];

/// The user's core rows (spec 11, "export everything"): every table with their UserId, then the
/// rows hanging off those or off their owned courses, listed table by table.
pub async fn core_part(conn: &mut MySqlConnection, user_id: i64) -> AppResult<Value> {
    let mut out = serde_json::Map::new();
    out.insert("Users".into(), json!(repo::dump(conn, "Users", "Id = ?", &[user_id]).await?));
    for t in repo::user_tables(conn).await? {
        out.insert(t.clone(), json!(repo::dump(conn, &t, "UserId = ?", &[user_id]).await?));
    }
    // Each `?` is the user's id.
    let owned = "SELECT Id FROM Course WHERE OwnerUserId = ?";
    let components = format!("SELECT Id FROM CourseComponent WHERE CourseId IN ({owned})");
    // Questions and cards: the owned courses' own and the user's private ones, never another user's.
    let mine = format!("(CourseId IN ({owned}) AND OwnerUserId IS NULL) OR OwnerUserId = ?");
    let versions = format!("SELECT Id FROM QuestionVersion WHERE QuestionId IN (SELECT Id FROM Question WHERE {mine})");
    let deadlines = "SELECT Id FROM Deadline WHERE CourseSubscriptionId IN (SELECT Id FROM CourseSubscription WHERE UserId = ?)";
    let rows: Vec<(&str, String, usize)> = vec![
        ("Course", "OwnerUserId = ?".into(), 1),
        ("GradeScale", "OwnerUserId = ?".into(), 1),
        ("Invite", "InvitedBy = ?".into(), 1),
        ("CourseShareInvite", "InvitedBy = ?".into(), 1),
        ("CourseComponent", format!("CourseId IN ({owned})"), 1),
        ("ComponentDependency", format!("DependantId IN ({components})"), 1),
        ("Concept", format!("CourseId IN ({owned})"), 1),
        ("Paper", format!("CourseId IN ({owned})"), 1),
        ("PaperOmission", format!("PaperId IN (SELECT Id FROM Paper WHERE CourseId IN ({owned}))"), 1),
        ("ComponentMaterial", format!("CourseComponentId IN ({components})"), 1),
        ("MaterialQuestion", format!("MaterialId IN (SELECT Id FROM ComponentMaterial WHERE CourseComponentId IN ({components}))"), 1),
        ("Question", mine.clone(), 2),
        ("QuestionConcept", format!("QuestionId IN (SELECT Id FROM Question WHERE {mine})"), 2),
        ("QuestionVersion", format!("QuestionId IN (SELECT Id FROM Question WHERE {mine})"), 2),
        ("MCQuestion", format!("QuestionVersionId IN ({versions})"), 2),
        ("MCOption", format!("QuestionVersionId IN ({versions})"), 2),
        ("NumericQuestion", format!("QuestionVersionId IN ({versions})"), 2),
        ("OpenQuestion", format!("QuestionVersionId IN ({versions})"), 2),
        ("Flashcard", mine.clone(), 2),
        ("Deadline", format!("Id IN ({deadlines})"), 1),
        ("DeadlineComponent", format!("DeadlineId IN ({deadlines})"), 1),
        ("MockQuestion", "MockExamId IN (SELECT Id FROM MockExam WHERE UserId = ?)".into(), 1),
        ("WeakSpotCheck", "WeakSpotId IN (SELECT Id FROM WeakSpot WHERE UserId = ?)".into(), 1),
    ];
    for (table, filter, n) in rows {
        out.insert(table.into(), json!(repo::dump(conn, table, &filter, &vec![user_id; n]).await?));
    }
    Ok(Value::Object(out))
}

/// Job `account.export` (every 30 s): builds one queued export.
pub async fn run_exports(ctx: &Ctx) -> anyhow::Result<u64> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let Some(e) = repo::claim_export(&mut tx).await? else { return Ok(0) };
    tx.commit().await?;
    let now = ctx.clock.now();
    let result: AppResult<(String, i64, String)> = async {
        let mut conn = ctx.pool.acquire().await?;
        let user = crux_repo::users::by_id(&mut conn, e.user_id).await?.ok_or(AppError::NotFound)?;
        let core = core_part(&mut conn, e.user_id).await?;
        drop(conn);
        let mut parts = json!({ "core": core });
        for svc in ["ai", "notify"] {
            let token = ctx.keys.sign_service("core", svc, now.timestamp());
            parts[svc] = match ctx.exports.part(svc, &token, &user.public_id).await {
                Ok(v) => v,
                // ai off (spec 9.6) or down: the export says so instead of failing.
                Err(err) => json!({ "unavailable": true, "reason": err.to_string() }),
            };
        }
        let token = ctx.keys.sign_service("core", "files", now.timestamp());
        let built =
            ctx.exports.build(&token, &user.public_id, &e.public_id, &parts).await.map_err(|err| anyhow::anyhow!("files: {err}"))?;
        Ok((built.file_ref, built.size_bytes, user.public_id))
    }
    .await;
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    match result {
        Ok((file_ref, size, user_ref)) => {
            let expires = now + Duration::hours(EXPORT_HOURS);
            repo::finish_export(&mut tx, e.id, "Ready", Some(&file_ref), Some(size), now, Some(expires), None).await?;
            crate::events::emit(
                &mut tx,
                ctx,
                topics::NOTIFY_REQUESTED,
                Some(&user_ref),
                json!({
                    "userRef": user_ref, "kind": "account.export_ready", "dedupeKey": format!("account.export_ready:{}", e.public_id),
                    "dueAt": iso(now), "payload": { "exportId": e.public_id, "fileId": file_ref, "expiresAt": iso(expires) },
                }),
            )
            .await
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        }
        Err(err) => {
            tracing::error!(error = %err, export = %e.public_id, alert = true, "export failed");
            repo::finish_export(
                &mut tx,
                e.id,
                "Failed",
                None,
                None,
                now,
                None,
                Some(&err.to_string().chars().take(500).collect::<String>()),
            )
            .await?;
        }
    }
    tx.commit().await?;
    Ok(1)
}

// ---------- deletion ----------

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CourseDecision {
    pub course_id: String,
    /// transfer (to a member) or delete
    pub action: String,
    /// transfer: the member who becomes the owner
    pub to_user_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeletionInput {
    pub password: String,
    /// For every course you own that others use (spec 11). Courses only you use are deleted.
    #[serde(default)]
    pub courses: Vec<CourseDecision>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SharedCourse {
    pub course_id: String,
    pub name: String,
    /// Accepted members who could take it over.
    pub members: Vec<crate::courses::Ref>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeletionView {
    /// None, Pending, Purging, Done, Cancelled
    pub status: String,
    pub requested_at: Option<String>,
    /// When the deletion runs (14 days after the request); cancel before then.
    pub runs_at: Option<String>,
    /// Owned courses with members: each needs a decision.
    pub shared_courses: Vec<SharedCourse>,
}

async fn shared_courses(conn: &mut MySqlConnection, actor: &Actor) -> AppResult<Vec<SharedCourse>> {
    let mut out = Vec::new();
    for c in crux_repo::courses::list_for_actor(conn, actor.user_id, false).await? {
        if c.my_role != "Owner" {
            continue;
        }
        let members: Vec<_> = crux_repo::courses::members(conn, c.id)
            .await?
            .into_iter()
            .filter(|m| m.status == "Accepted")
            .map(|m| crate::courses::Ref { id: m.user_ref, name: m.username })
            .collect();
        if !members.is_empty() {
            out.push(SharedCourse { course_id: c.public_id.clone(), name: c.name.clone(), members });
        }
    }
    Ok(out)
}

pub async fn deletion_status(ctx: &Ctx, actor: &Actor) -> AppResult<DeletionView> {
    let mut conn = ctx.pool.acquire().await?;
    let row = repo::deletion(&mut conn, &actor.public_id).await?;
    let shared = shared_courses(&mut conn, actor).await?;
    Ok(DeletionView {
        status: row.as_ref().map(|r| r.status.clone()).unwrap_or_else(|| "None".into()),
        requested_at: row.as_ref().map(|r| iso(r.requested_at)),
        runs_at: row.as_ref().filter(|r| r.status == "Pending").map(|r| iso(r.requested_at + Duration::days(GRACE_DAYS))),
        shared_courses: shared,
    })
}

pub async fn request_deletion(ctx: &Ctx, actor: &Actor, input: DeletionInput) -> AppResult<DeletionView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let user = crux_repo::users::by_id(&mut tx, actor.user_id).await?.ok_or(AppError::NotFound)?;
    let ok = match &user.password_hash {
        Some(h) => crate::password::verify_blocking(input.password.clone(), h.clone()).await,
        None => false,
    };
    if !ok {
        return Err(ValidationErrors::single("password", "wrong", "That isn't your password.").into());
    }
    let shared = shared_courses(&mut tx, actor).await?;
    let mut e = ValidationErrors::new();
    let mut decisions = Vec::new();
    for c in &shared {
        match input.courses.iter().find(|d| d.course_id == c.course_id) {
            None => e.push(&format!("courses.{}", c.course_id), "required", "Choose: transfer it to a member, or delete it."),
            Some(d) if d.action == "delete" => decisions.push(json!({ "courseRef": c.course_id, "action": "delete" })),
            Some(d) if d.action == "transfer" => match d.to_user_id.as_ref().filter(|u| c.members.iter().any(|m| &&m.id == u)) {
                Some(u) => decisions.push(json!({ "courseRef": c.course_id, "action": "transfer", "toUserRef": u })),
                None => e.push(&format!("courses.{}", c.course_id), "member", "Transfer to one of the course's members."),
            },
            Some(_) => e.push(&format!("courses.{}", c.course_id), "invalid", "Use transfer or delete."),
        }
    }
    e.into_result()?;
    let now = ctx.clock.now();
    repo::upsert_deletion(&mut tx, &actor.public_id, actor.user_id, now, &json!(decisions)).await?;
    repo::set_user_status(&mut tx, actor.user_id, "PendingDeletion", Some(now)).await?;
    tx.commit().await?;
    deletion_status(ctx, actor).await
}

pub async fn cancel_deletion(ctx: &Ctx, actor: &Actor) -> AppResult<DeletionView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    match repo::deletion(&mut tx, &actor.public_id).await? {
        Some(r) if r.status == "Pending" => {
            repo::set_deletion_status(&mut tx, &actor.public_id, "Cancelled", ctx.clock.now()).await?;
            repo::set_user_status(&mut tx, actor.user_id, "Active", None).await?;
        }
        _ => return Err(AppError::conflict("There's no pending deletion to cancel.")),
    }
    tx.commit().await?;
    deletion_status(ctx, actor).await
}

/// Job `account.delete` (spec 10, daily; run hourly here): the grace period is over.
pub async fn run_deletions(ctx: &Ctx) -> anyhow::Result<u64> {
    let now = ctx.clock.now();
    let mut conn = ctx.pool.acquire().await?;
    let due = repo::due_deletions(&mut conn, now - Duration::days(GRACE_DAYS)).await?;
    drop(conn);
    let mut n = 0;
    for user_ref in due {
        let mut tx = crux_repo::begin(&ctx.pool).await?;
        let Some(d) = repo::deletion(&mut tx, &user_ref).await? else { continue };
        if d.status != "Pending" {
            continue;
        }
        for dec in d.course_decisions.0.as_array().cloned().unwrap_or_default() {
            let Some(cref) = dec["courseRef"].as_str() else { continue };
            let Some((_, course_id)) = crux_repo::courses::owner_of_live_course(&mut tx, cref).await? else { continue };
            // Audit M3: the choice is up to 14 days old. Transfer only to someone who is still an
            // accepted member with an active account; otherwise the course goes with the account.
            if dec["action"] == "transfer" {
                if let Some(to) = dec["toUserRef"].as_str() {
                    let member = crux_repo::courses::members(&mut tx, course_id)
                        .await?
                        .into_iter()
                        .any(|m| m.user_ref == to && m.status == "Accepted");
                    if let Some(u) = crux_repo::users::by_public_id(&mut tx, to).await?.filter(|u| member && u.status == "Active") {
                        repo::transfer_course(&mut tx, course_id, d.user_id, u.id).await?;
                        continue;
                    }
                }
            }
            delete_course(&mut tx, ctx, course_id, cref, &user_ref).await?;
        }
        // Courses nobody else uses go with the account.
        let left: Vec<(i64, String)> = sqlx_owned_courses(&mut tx, d.user_id).await?;
        for (course_id, cref) in left {
            delete_course(&mut tx, ctx, course_id, &cref, &user_ref).await?;
        }
        crate::events::emit(&mut tx, ctx, topics::USER_DELETED, Some(&user_ref), json!({ "userRef": user_ref }))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        repo::set_deletion_status(&mut tx, &user_ref, "Purging", now).await?;
        repo::set_acks(&mut tx, &user_ref, &json!([])).await?;
        // Audit M3: no more sign-ins or writes while the services purge (they'd be orphaned).
        repo::set_user_status(&mut tx, d.user_id, "Disabled", Some(d.requested_at)).await?;
        crux_repo::sessions::revoke_all_for_user(&mut tx, d.user_id, now, crux_domain::auth::RevokeReason::Admin).await?;
        tx.commit().await?;
        n += 1;
    }
    let mut conn = ctx.pool.acquire().await?;
    for (user_ref, user_id, acks) in repo::stalled_purges(&mut conn, now - Duration::days(ACK_TIMEOUT_DAYS)).await? {
        let acks: Vec<String> = acks.and_then(|a| serde_json::from_value(a.0).ok()).unwrap_or_default();
        let missing: Vec<&str> = ACK_SERVICES.iter().copied().filter(|s| !acks.iter().any(|a| a == s)).collect();
        tracing::error!(alert = true, user = %user_ref, missing = ?missing, "account deletion finished without every service's ack");
        repo::delete_user(&mut conn, user_id).await?;
        repo::set_deletion_status(&mut conn, &user_ref, "Done", now).await?;
        n += 1;
    }
    Ok(n)
}

async fn sqlx_owned_courses(conn: &mut MySqlConnection, user_id: i64) -> anyhow::Result<Vec<(i64, String)>> {
    Ok(crux_repo::courses::list_for_actor(conn, user_id, false)
        .await?
        .into_iter()
        .chain(crux_repo::courses::list_for_actor(conn, user_id, true).await?)
        .filter(|c| c.my_role == "Owner")
        .map(|c| (c.id, c.public_id))
        .collect())
}

async fn delete_course(conn: &mut MySqlConnection, ctx: &Ctx, course_id: i64, course_ref: &str, owner_ref: &str) -> anyhow::Result<()> {
    crux_repo::courses::set_archived(conn, course_id, Some(ctx.clock.now())).await?;
    let color = crux_repo::courses::owner_ref_and_color(conn, course_id).await?.map(|(_, c)| c).unwrap_or_default();
    crux_repo::courses::hard_delete(conn, course_id).await?;
    crate::events::emit(
        conn,
        ctx,
        topics::COURSE_DELETED,
        Some(course_ref),
        json!({ "courseRef": course_ref, "ownerRef": owner_ref, "memberRefs": [], "color": color }),
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))
}

/// `user.purged` (consumer): one service is done; when all are, the user's core rows go.
pub async fn on_user_purged(conn: &mut MySqlConnection, payload: &Value) -> anyhow::Result<()> {
    crate::contracts::validate("user.purged", 1, payload).map_err(|e| anyhow::anyhow!(e))?;
    let user_ref = payload["userRef"].as_str().unwrap_or_default();
    let service = payload["service"].as_str().unwrap_or_default();
    let Some(d) = repo::deletion(conn, user_ref).await? else { return Ok(()) };
    if d.status != "Purging" {
        return Ok(());
    }
    let mut acks: Vec<String> = d.acks.map(|a| serde_json::from_value(a.0).unwrap_or_default()).unwrap_or_default();
    if !acks.iter().any(|a| a == service) {
        acks.push(service.to_string());
    }
    repo::set_acks(conn, user_ref, &json!(acks)).await?;
    if ACK_SERVICES.iter().all(|s| acks.iter().any(|a| a == s)) {
        repo::delete_user(conn, d.user_id).await?;
        repo::set_deletion_status(conn, user_ref, "Done", Utc::now()).await?;
    }
    Ok(())
}
