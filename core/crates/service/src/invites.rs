//! Invites (spec 0: invite-only launch). Users need a verified email to invite (spec 4.1); admins
//! can pick any tier and any number of uses through `/admin/invites`.

use chrono::Duration;
use crux_domain::auth::{normalize_email, normalize_invite_code, token_hash, validate_email};
use crux_domain::page::{clamp_limit, decode_cursor, encode_cursor};
use crux_domain::ValidationErrors;
use crux_repo::invites::{self, NewInvite};
use crux_repo::tiers;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::views::{CreatedInvite, InviteView, Page};
use crate::{Actor, AppError, AppResult, Ctx};

/// Limits for invites made by ordinary users. Not in the spec; see OPEN-QUESTIONS.md.
pub const USER_MAX_USES: i32 = 5;
pub const USER_MAX_DAYS: i64 = 30;
pub const DEFAULT_DAYS: i64 = 14;

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateInviteInput {
    /// Only this address can use the invite.
    pub email: Option<String>,
    pub max_uses: Option<i32>,
    pub expires_in_days: Option<i64>,
    /// Admin only: the tier the new user gets (default tier otherwise).
    pub tier_code: Option<String>,
}

fn validate(input: &CreateInviteInput, as_admin: bool) -> ValidationErrors {
    let mut e = ValidationErrors::new();
    if let Some(email) = &input.email {
        e.merge(validate_email(email));
    }
    let max_uses_cap = if as_admin { 10_000 } else { USER_MAX_USES };
    if input.max_uses.is_some_and(|m| !(1..=max_uses_cap).contains(&m)) {
        e.push("maxUses", "invalid", &format!("Use 1-{max_uses_cap}."));
    }
    let days_cap = if as_admin { 365 } else { USER_MAX_DAYS };
    if input.expires_in_days.is_some_and(|d| !(1..=days_cap).contains(&d)) {
        e.push("expiresInDays", "invalid", &format!("Use 1-{days_cap} days."));
    }
    if input.tier_code.is_some() && !as_admin {
        e.push("tierCode", "forbidden", "Only admins can choose a tier.");
    }
    e
}

pub async fn create(ctx: &Ctx, actor: &Actor, input: CreateInviteInput, as_admin: bool) -> AppResult<CreatedInvite> {
    if as_admin && !actor.is_admin() {
        return Err(AppError::NotFound);
    }
    if !as_admin && !actor.email_verified {
        return Err(AppError::EmailUnverified);
    }
    validate(&input, as_admin).into_result()?;

    let now = ctx.clock.now();
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let tier_id = match &input.tier_code {
        Some(code) => Some(
            tiers::by_code(&mut tx, code)
                .await?
                .filter(|t| t.is_active)
                .ok_or_else(|| ValidationErrors::single("tierCode", "unknown", "No active tier with that code."))?
                .id,
        ),
        None => None,
    };
    let code = ctx.ids.invite_code();
    let public_id = ctx.ids.ulid();
    let email = input.email.as_deref().map(normalize_email);
    let id = invites::insert(
        &mut tx,
        &NewInvite {
            public_id: &public_id,
            code_hash: &token_hash(&normalize_invite_code(&code)),
            email: email.as_deref(),
            invited_by: Some(actor.user_id),
            tier_id,
            max_uses: input.max_uses.unwrap_or(1),
            expires_at: Some(now + Duration::days(input.expires_in_days.unwrap_or(DEFAULT_DAYS))),
        },
    )
    .await?;
    let row = invites::by_public_id(&mut tx, &public_id, None).await?.ok_or_else(|| anyhow::anyhow!("invite {id} vanished"))?;
    tx.commit().await?;
    Ok(CreatedInvite { invite: InviteView::from(&row), signup_url: format!("{}/signup?invite={code}", ctx.config.app_base_url), code })
}

/// `as_admin` lists everyone's invites; otherwise only the actor's own.
pub async fn list(ctx: &Ctx, actor: &Actor, cursor: Option<&str>, limit: Option<u32>, as_admin: bool) -> AppResult<Page<InviteView>> {
    if as_admin && !actor.is_admin() {
        return Err(AppError::NotFound);
    }
    let limit = clamp_limit(limit);
    let before = cursor.and_then(decode_cursor);
    let inviter = if as_admin { None } else { Some(actor.user_id) };
    let mut conn = ctx.pool.acquire().await?;
    let rows = invites::list(&mut conn, inviter, before, limit + 1).await?;
    let next_cursor = (rows.len() > limit as usize).then(|| encode_cursor(rows[limit as usize - 1].id));
    Ok(Page { items: rows.iter().take(limit as usize).map(InviteView::from).collect(), next_cursor })
}

pub async fn revoke(ctx: &Ctx, actor: &Actor, invite_id: &str, as_admin: bool) -> AppResult<()> {
    if as_admin && !actor.is_admin() {
        return Err(AppError::NotFound);
    }
    let inviter = if as_admin { None } else { Some(actor.user_id) };
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let row = invites::by_public_id(&mut tx, invite_id, inviter).await?.ok_or(AppError::NotFound)?;
    invites::revoke(&mut tx, row.id, ctx.clock.now()).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_limits_are_tighter_than_admin_limits() {
        let input = CreateInviteInput { max_uses: Some(50), expires_in_days: Some(90), ..Default::default() };
        assert_eq!(validate(&input, false).0.len(), 2);
        assert!(validate(&input, true).is_empty());
    }

    #[test]
    fn only_admins_pick_tiers() {
        let input = CreateInviteInput { tier_code: Some("plus".into()), ..Default::default() };
        assert_eq!(validate(&input, false).0[0].field, "tierCode");
        assert!(validate(&input, true).is_empty());
    }

    #[test]
    fn email_is_checked() {
        let input = CreateInviteInput { email: Some("nope".into()), ..Default::default() };
        assert_eq!(validate(&input, true).0[0].field, "email");
    }
}
