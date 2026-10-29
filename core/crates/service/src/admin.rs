//! `/admin/*` (spec 4.2, 4.3). Admin acts only through these. Every read or write of another
//! user's data writes a `UserEvent` with `ActorUserId`.

use crux_domain::page::{clamp_limit, decode_cursor, encode_cursor};
use crux_domain::tier::validate_tier;
use crux_domain::{topics, ValidationErrors};
use crux_repo::tiers::{self, TierWrite};
use crux_repo::user_events::{self, NewUserEvent};
use crux_repo::{users, MySqlConnection, RepoError};
use serde::Deserialize;
use serde_json::json;
use utoipa::ToSchema;

use crate::events::{emit, user_payload};
use crate::views::{Page, TierView, UserView};
use crate::{Actor, AppError, AppResult, Ctx};

fn require_admin(actor: &Actor) -> AppResult<()> {
    if actor.is_admin() {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

async fn audit(
    conn: &mut MySqlConnection,
    ctx: &Ctx,
    actor: &Actor,
    subject_user_id: i64,
    kind: &str,
    payload: serde_json::Value,
) -> AppResult<()> {
    user_events::insert(
        conn,
        &NewUserEvent {
            user_id: subject_user_id,
            actor_user_id: Some(actor.user_id),
            client_id: None,
            occurred_at: ctx.clock.now(),
            event_type: kind,
            course_id: None,
            component_id: None,
            entity_type: Some("User"),
            entity_id: Some(subject_user_id),
            payload: Some(payload),
        },
    )
    .await?;
    Ok(())
}

pub async fn list_users(ctx: &Ctx, actor: &Actor, q: Option<&str>, cursor: Option<&str>, limit: Option<u32>) -> AppResult<Page<UserView>> {
    require_admin(actor)?;
    let limit = clamp_limit(limit);
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let rows = users::list(&mut tx, cursor.and_then(decode_cursor), limit + 1, q.filter(|s| !s.trim().is_empty())).await?;
    let next_cursor = (rows.len() > limit as usize).then(|| encode_cursor(rows[limit as usize - 1].id));
    let rows: Vec<_> = rows.into_iter().take(limit as usize).collect();
    for u in rows.iter().filter(|u| u.id != actor.user_id) {
        audit(&mut tx, ctx, actor, u.id, "admin.user_read", json!({ "via": "admin.users.list" })).await?;
    }
    tx.commit().await?;
    Ok(Page { items: rows.iter().map(UserView::from).collect(), next_cursor })
}

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserPatch {
    pub tier_code: Option<String>,
    /// `User` or `Admin`.
    pub role: Option<String>,
    /// `Active` or `Disabled`. Deletion goes through the account flow, not here.
    pub status: Option<String>,
    /// Free text kept on `UserTierChange.Reason`.
    pub reason: Option<String>,
}

pub async fn patch_user(ctx: &Ctx, actor: &Actor, user_ref: &str, patch: AdminUserPatch) -> AppResult<UserView> {
    require_admin(actor)?;
    let mut e = ValidationErrors::new();
    if patch.role.as_deref().is_some_and(|r| !matches!(r, "User" | "Admin")) {
        e.push("role", "invalid", "Use User or Admin.");
    }
    if patch.status.as_deref().is_some_and(|s| !matches!(s, "Active" | "Disabled")) {
        e.push("status", "invalid", "Use Active or Disabled.");
    }
    if patch.reason.as_deref().is_some_and(|r| r.chars().count() > 255) {
        e.push("reason", "invalid", "At most 255 characters.");
    }
    e.into_result()?;

    let now = ctx.clock.now();
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let target = users::by_public_id(&mut tx, user_ref).await?.ok_or(AppError::NotFound)?;
    if target.id == actor.user_id && (patch.role.as_deref() == Some("User") || patch.status.as_deref() == Some("Disabled")) {
        return Err(ValidationErrors::single("role", "self", "You can't demote or disable your own account.").into());
    }

    if let Some(code) = &patch.tier_code {
        let tier = tiers::by_code(&mut tx, code)
            .await?
            .ok_or_else(|| ValidationErrors::single("tierCode", "unknown", "No tier with that code."))?;
        if tier.id != target.tier_id {
            users::set_tier(&mut tx, target.id, tier.id).await?;
            let reason = patch.reason.as_deref().unwrap_or("admin");
            tiers::record_change(&mut tx, target.id, Some(target.tier_id), tier.id, Some(actor.user_id), reason, now).await?;
            emit(
                &mut tx,
                ctx,
                topics::USER_TIER_CHANGED,
                Some(&target.public_id),
                json!({ "userRef": target.public_id, "tierCode": tier.code, "storageQuotaBytes": tier.storage_quota_bytes }),
            )
            .await?;
        }
    }
    // Audit M3: a status change settles a pending deletion first (Active cancels it), and an
    // account that's already being purged can't be brought back.
    if patch.status.is_some() {
        if let Some(d) = crux_repo::account::deletion(&mut tx, &target.public_id).await? {
            if d.status == "Purging" {
                return Err(AppError::conflict("This account is being deleted."));
            }
            if d.status == "Pending" && patch.status.as_deref() == Some("Active") {
                crux_repo::account::set_deletion_status(&mut tx, &target.public_id, "Cancelled", now).await?;
            }
        }
    }
    if patch.role.is_some() || patch.status.is_some() {
        users::set_role_status(&mut tx, target.id, patch.role.as_deref(), patch.status.as_deref()).await?;
        if patch.status.as_deref() == Some("Disabled") {
            crux_repo::sessions::revoke_all_for_user(&mut tx, target.id, now, crux_domain::auth::RevokeReason::Admin).await?;
        }
        // Audit L1: other services stop honouring the old role or a disabled account at once.
        crate::auth::revoke_access_tokens(ctx, &target.public_id).await?;
    }
    audit(
        &mut tx,
        ctx,
        actor,
        target.id,
        "admin.user_update",
        json!({ "tierCode": patch.tier_code, "role": patch.role, "status": patch.status }),
    )
    .await?;
    let u = users::by_id(&mut tx, target.id).await?.ok_or(AppError::NotFound)?;
    let tier = tiers::by_id(&mut tx, u.tier_id).await?.ok_or(AppError::NotFound)?;
    emit(&mut tx, ctx, topics::USER_UPDATED, Some(&u.public_id), user_payload(&u, tier.storage_quota_bytes)).await?;
    tx.commit().await?;
    Ok(UserView::from(&u))
}

pub async fn list_tiers(ctx: &Ctx, actor: &Actor) -> AppResult<Vec<TierView>> {
    require_admin(actor)?;
    let mut conn = ctx.pool.acquire().await?;
    Ok(tiers::list(&mut conn).await?.iter().map(TierView::from).collect())
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TierInput {
    pub code: String,
    pub name: String,
    pub storage_quota_bytes: i64,
    pub max_courses: Option<i32>,
    #[schema(value_type = Option<Object>)]
    pub limits: Option<serde_json::Value>,
    #[serde(default = "yes")]
    pub is_active: bool,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub sort_order: i32,
}

fn yes() -> bool {
    true
}

fn tier_write(t: &TierInput) -> TierWrite<'_> {
    TierWrite {
        code: &t.code,
        name: t.name.trim(),
        storage_quota_bytes: t.storage_quota_bytes,
        max_courses: t.max_courses,
        limits: t.limits.as_ref(),
        is_active: t.is_active,
        sort_order: t.sort_order,
    }
}

fn duplicate_code(e: RepoError) -> AppError {
    match e {
        RepoError::Duplicate(_) => ValidationErrors::single("code", "taken", "A tier with this code exists.").into(),
        other => other.into(),
    }
}

pub async fn create_tier(ctx: &Ctx, actor: &Actor, input: TierInput) -> AppResult<TierView> {
    require_admin(actor)?;
    validate_tier(&input.code, &input.name, input.storage_quota_bytes, input.max_courses).into_result()?;
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let id = tiers::insert(&mut tx, &tier_write(&input)).await.map_err(duplicate_code)?;
    if input.is_default {
        tiers::make_default(&mut tx, id).await?;
    }
    let t = tiers::by_id(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(TierView::from(&t))
}

/// Updating a tier's quota re-announces `user.tier_changed` for every user on it, so files picks
/// up the new quota (OPEN-QUESTIONS.md: fine at v1 scale, batch later).
pub async fn update_tier(ctx: &Ctx, actor: &Actor, code: &str, input: TierInput) -> AppResult<TierView> {
    require_admin(actor)?;
    validate_tier(&input.code, &input.name, input.storage_quota_bytes, input.max_courses).into_result()?;
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let cur = tiers::by_code(&mut tx, code).await?.ok_or(AppError::NotFound)?;
    if cur.is_default && !input.is_default {
        return Err(ValidationErrors::single("isDefault", "required", "Make another tier the default first.").into());
    }
    tiers::update(&mut tx, cur.id, &tier_write(&input)).await.map_err(duplicate_code)?;
    if input.is_default && !cur.is_default {
        tiers::make_default(&mut tx, cur.id).await?;
    }
    if cur.storage_quota_bytes != input.storage_quota_bytes || cur.code != input.code {
        for user_ref in users::public_ids_on_tier(&mut tx, cur.id).await? {
            emit(
                &mut tx,
                ctx,
                topics::USER_TIER_CHANGED,
                Some(&user_ref),
                json!({ "userRef": user_ref, "tierCode": input.code, "storageQuotaBytes": input.storage_quota_bytes }),
            )
            .await?;
        }
    }
    let t = tiers::by_id(&mut tx, cur.id).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(TierView::from(&t))
}
