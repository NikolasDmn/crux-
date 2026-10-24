//! Storage saver, core side (backend-spec 6.2): the course owner frees up space per material or per
//! course, can undo within 7 days, and core mirrors what files reports on the materials.

use chrono::{DateTime, Duration, Utc};
use crux_domain::policy::can_manage_course;
use crux_domain::topics;
use crux_domain::ValidationErrors;
use crux_repo::catalog as repo;
use crux_repo::MySqlConnection;
use serde::Serialize;
use serde_json::json;
use utoipa::ToSchema;

use crate::catalog::{material_for, MaterialView};
use crate::courses::{access_of, manageable};
use crate::events::emit;
use crate::time::iso;
use crate::{Actor, AppError, AppResult, Ctx};

/// Spec 6.2: the undo window.
pub const UNDO_DAYS: i64 = 7;

async fn reload(conn: &mut MySqlConnection, material_ref: &str) -> AppResult<MaterialView> {
    Ok(MaterialView::from(&repo::material(conn, material_ref).await?.ok_or(AppError::NotFound)?))
}

/// `POST /materials/{id}/free-space` (course Owner only): the original goes in 7 days.
pub async fn free_material(ctx: &Ctx, actor: &Actor, material_ref: &str) -> AppResult<MaterialView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let (m, course) = material_for(&mut tx, actor, material_ref, true).await?;
    if !can_manage_course(&access_of(&course)) {
        return Err(AppError::NotFound);
    }
    let Some(file_ref) = m.file_ref.clone() else {
        return Err(ValidationErrors::single("fileId", "no_original", "This material has no uploaded file.").into());
    };
    match m.original_status.as_str() {
        "PendingPurge" => return reload(&mut tx, material_ref).await,
        "Purged" => return Err(ValidationErrors::single("fileId", "already_purged", "The original is already removed.").into()),
        _ => {}
    }
    let purge_at = ctx.clock.now() + Duration::days(UNDO_DAYS);
    request(&mut tx, ctx, &file_ref, &course.owner_ref, &course.public_id, Some(&m.public_id), purge_at).await?;
    let view = reload(&mut tx, material_ref).await?;
    tx.commit().await?;
    Ok(view)
}

async fn request(
    conn: &mut MySqlConnection,
    ctx: &Ctx,
    file_ref: &str,
    owner_ref: &str,
    course_ref: &str,
    material_ref: Option<&str>,
    purge_at: DateTime<Utc>,
) -> AppResult<()> {
    repo::set_original_status_for_file(conn, file_ref, &["Present"], "PendingPurge", Some(purge_at)).await?;
    let mut payload = json!({ "fileRef": file_ref, "ownerRef": owner_ref, "courseRef": course_ref, "purgeAt": iso(purge_at) });
    if let Some(m) = material_ref {
        payload["materialRef"] = json!(m);
    }
    emit(conn, ctx, topics::MATERIAL_PURGE_REQUESTED, Some(file_ref), payload).await
}

/// `POST /materials/{id}/free-space/cancel`: undo before the purge runs.
pub async fn cancel_material(ctx: &Ctx, actor: &Actor, material_ref: &str) -> AppResult<MaterialView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let (m, course) = material_for(&mut tx, actor, material_ref, true).await?;
    if !can_manage_course(&access_of(&course)) {
        return Err(AppError::NotFound);
    }
    let pending = m.original_status == "PendingPurge" && m.original_purge_at.is_some_and(|t| t > ctx.clock.now());
    let Some(file_ref) = m.file_ref.clone().filter(|_| pending) else {
        return Err(AppError::conflict("There's no pending removal to undo."));
    };
    repo::set_original_status_for_file(&mut tx, &file_ref, &["PendingPurge"], "Present", None).await?;
    emit(
        &mut tx,
        ctx,
        topics::MATERIAL_PURGE_CANCELLED,
        Some(&file_ref),
        json!({ "fileRef": file_ref, "ownerRef": course.owner_ref, "courseRef": course.public_id, "materialRef": m.public_id }),
    )
    .await?;
    let view = reload(&mut tx, material_ref).await?;
    tx.commit().await?;
    Ok(view)
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FreeSpaceResult {
    /// Files whose original is now set to go (materials and past papers).
    pub files: usize,
    pub purge_at: String,
}

/// `POST /courses/{id}/free-space`: every material with a file, and past-paper files.
pub async fn free_course(ctx: &Ctx, actor: &Actor, course_ref: &str) -> AppResult<FreeSpaceResult> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let course = manageable(&mut tx, actor, course_ref).await?;
    let purge_at = ctx.clock.now() + Duration::days(UNDO_DAYS);
    let mut files: Vec<(String, Option<String>)> = Vec::new();
    for m in repo::course_materials_with_file(&mut tx, course.id).await? {
        if let Some(f) = m.file_ref.clone() {
            if !files.iter().any(|(x, _)| *x == f) {
                files.push((f, Some(m.public_id.clone())));
            }
        }
    }
    for f in repo::course_paper_files(&mut tx, course.id).await? {
        if !files.iter().any(|(x, _)| *x == f) {
            files.push((f, None));
        }
    }
    for (f, m) in &files {
        request(&mut tx, ctx, f, &course.owner_ref, &course.public_id, m.as_deref(), purge_at).await?;
    }
    tx.commit().await?;
    Ok(FreeSpaceResult { files: files.len(), purge_at: iso(purge_at) })
}

/// What files reports (consumer, spec 6.2): its purge state mirrored on every material of the file.
pub async fn on_file_event(conn: &mut MySqlConnection, topic: &str, payload: &serde_json::Value) -> anyhow::Result<()> {
    crate::contracts::validate(topic, 1, payload).map_err(|e| anyhow::anyhow!(e))?;
    let file_ref = payload["fileRef"].as_str().unwrap_or_default();
    match topic {
        topics::FILE_PURGE_SCHEDULED => {
            let at = payload["purgeAt"].as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|t| t.with_timezone(&Utc));
            repo::set_original_status_for_file(conn, file_ref, &["Present"], "PendingPurge", at).await?;
        }
        topics::FILE_PURGED => {
            repo::set_original_status_for_file(conn, file_ref, &["Present", "PendingPurge"], "Purged", None).await?;
        }
        topics::FILE_RESTORED => {
            repo::set_original_status_for_file(conn, file_ref, &["Purged", "PendingPurge"], "Present", None).await?;
        }
        _ => {}
    }
    Ok(())
}
