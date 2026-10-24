//! `/me`, `/me/settings`, `/me/tier` (owns).

use crux_domain::topics;
use crux_domain::user::{validate_profile_patch, validate_settings_patch, ProfilePatch, SettingsPatch};
use crux_domain::ValidationErrors;
use crux_repo::settings::SettingsRow;
use crux_repo::sqlx_reexport::Json;
use crux_repo::{settings, tiers, users, RepoError};

use crate::events::{emit, user_payload};
use crate::views::{SettingsView, TierView, UserView};
use crate::{Actor, AppError, AppResult, Ctx};

pub async fn get(ctx: &Ctx, actor: &Actor) -> AppResult<UserView> {
    let mut conn = ctx.pool.acquire().await?;
    let u = users::by_id(&mut conn, actor.user_id).await?.ok_or(AppError::NotFound)?;
    Ok(UserView::from(&u))
}

/// `PATCH /me` with `If-Match`. A conflict returns the current resource (spec 3.1).
pub async fn patch(ctx: &Ctx, actor: &Actor, version: i32, patch: ProfilePatch) -> AppResult<UserView> {
    validate_profile_patch(&patch).into_result()?;
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let cur = users::by_id(&mut tx, actor.user_id).await?.ok_or(AppError::NotFound)?;
    if cur.row_version != version {
        return Err(AppError::Conflict {
            message: "Changed somewhere else.".into(),
            current: Some(serde_json::to_value(UserView::from(&cur)).unwrap_or_default()),
        });
    }
    let update = users::ProfileUpdate {
        username: patch.username.as_deref().map(str::trim).unwrap_or(&cur.username),
        first_name: patch.first_name.as_deref().map(str::trim).unwrap_or(&cur.first_name),
        last_name: patch.last_name.as_deref().map(str::trim).unwrap_or(&cur.last_name),
        time_zone: patch.time_zone.as_deref().unwrap_or(&cur.time_zone),
    };
    users::update_profile(&mut tx, cur.id, version, &update).await.map_err(|e| match e {
        RepoError::Duplicate(_) => ValidationErrors::single("username", "taken", "That username is taken.").into(),
        other => AppError::from(other),
    })?;
    let u = users::by_id(&mut tx, cur.id).await?.ok_or(AppError::NotFound)?;
    let tier = tiers::by_id(&mut tx, u.tier_id).await?.ok_or(AppError::NotFound)?;
    emit(&mut tx, ctx, topics::USER_UPDATED, Some(&u.public_id), user_payload(&u, tier.storage_quota_bytes)).await?;
    tx.commit().await?;
    Ok(UserView::from(&u))
}

pub async fn get_settings(ctx: &Ctx, actor: &Actor) -> AppResult<SettingsView> {
    let mut conn = ctx.pool.acquire().await?;
    let s = settings::get(&mut conn, actor.user_id).await?.ok_or(AppError::NotFound)?;
    Ok(SettingsView::from(&s))
}

pub async fn patch_settings(ctx: &Ctx, actor: &Actor, version: i32, patch: SettingsPatch) -> AppResult<SettingsView> {
    validate_settings_patch(&patch).into_result()?;
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let cur = settings::get(&mut tx, actor.user_id).await?.ok_or(AppError::NotFound)?;
    if cur.row_version != version {
        return Err(AppError::Conflict {
            message: "Changed somewhere else.".into(),
            current: Some(serde_json::to_value(SettingsView::from(&cur)).unwrap_or_default()),
        });
    }
    let next = SettingsRow {
        flash_retention: patch.flash_retention.unwrap_or(cur.flash_retention),
        flash_new_per_day: patch.flash_new_per_day.unwrap_or(cur.flash_new_per_day),
        flash_review_limit: patch.flash_review_limit.unwrap_or(cur.flash_review_limit),
        mock_go_easy: patch.mock_go_easy.unwrap_or(cur.mock_go_easy),
        mock_real_layout: patch.mock_real_layout.unwrap_or(cur.mock_real_layout),
        extra: patch.extra.map(Json).or(cur.extra.clone()),
        ..cur.clone()
    };
    settings::update(&mut tx, actor.user_id, version, &next).await?;
    // The planner reads these from Extra: a change re-lays the plan (pinned entries stay).
    let planner = |e: &Option<Json<serde_json::Value>>| {
        ["dailyHours", "multiCoursePerDay", "studyStart", "studyEnd"].map(|k| e.as_ref().and_then(|j| j.0.get(k).cloned()))
    };
    if planner(&cur.extra) != planner(&next.extra) {
        crate::plan::rebuild_in(&mut tx, ctx, actor.user_id).await?;
    }
    let s = settings::get(&mut tx, actor.user_id).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(SettingsView::from(&s))
}

pub async fn tier(ctx: &Ctx, actor: &Actor) -> AppResult<TierView> {
    let mut conn = ctx.pool.acquire().await?;
    let u = users::by_id(&mut conn, actor.user_id).await?.ok_or(AppError::NotFound)?;
    let t = tiers::by_id(&mut conn, u.tier_id).await?.ok_or(AppError::NotFound)?;
    Ok(TierView::from(&t))
}
