//! Signup, login, sessions, email verification, password reset (spec 4.1).

use chrono::{DateTime, Utc};
use crux_domain::auth::{self as rules, InviteRejection, RefreshDecision};
use crux_domain::tier::signup_tier_id;
use crux_domain::user::{Role, UserStatus};
use crux_domain::{topics, ValidationErrors};
use crux_repo::auth_tokens::{self, Purpose};
use crux_repo::users::{self, NewUser, UserRow};
use crux_repo::{invites, sessions, settings, tiers, RepoError};
use serde::Deserialize;
use serde_json::json;
use utoipa::ToSchema;

use crate::events::{emit, request_account_email, user_payload};
use crate::jwt::{AccessClaims, ACCESS_AUDIENCE, ISSUER};
use crate::ratelimit;
use crate::views::UserView;
use crate::{password, Actor, AppError, AppResult, Ctx};

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignupInput {
    pub invite_code: String,
    pub email: String,
    pub username: String,
    pub first_name: String,
    pub last_name: String,
    pub password: String,
    pub time_zone: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginInput {
    /// Email or username.
    pub login: String,
    pub password: String,
}

/// What a successful signup/login/refresh hands to the http layer: the access token goes in the
/// body, the refresh token into the cookie.
#[derive(Debug, Clone)]
pub struct Session {
    pub access_token: String,
    pub access_expires_at: DateTime<Utc>,
    pub refresh_token: String,
    pub refresh_expires_at: DateTime<Utc>,
    pub user: UserView,
}

/// Request metadata the auth flows use for rate limits and session labels.
#[derive(Debug, Clone, Default)]
pub struct ClientInfo {
    pub ip: String,
    pub user_agent: Option<String>,
}

fn invite_reason(r: InviteRejection) -> &'static str {
    match r {
        InviteRejection::Revoked => "revoked",
        InviteRejection::Expired => "expired",
        InviteRejection::UsedUp => "used_up",
        InviteRejection::WrongEmail => "wrong_email",
    }
}

fn duplicate_to_validation(e: RepoError) -> AppError {
    match e {
        RepoError::Duplicate(key) if key.contains("username") => {
            ValidationErrors::single("username", "taken", "That username is taken.").into()
        }
        RepoError::Duplicate(key) if key.contains("email") => {
            ValidationErrors::single("email", "taken", "An account with this email already exists.").into()
        }
        other => other.into(),
    }
}

pub(crate) fn issue_access(ctx: &Ctx, u: &UserRow) -> (String, DateTime<Utc>) {
    let now = ctx.clock.now();
    let exp = now + rules::ACCESS_TOKEN_TTL;
    let token = ctx.keys.sign_access(&AccessClaims {
        iss: ISSUER.into(),
        aud: ACCESS_AUDIENCE.into(),
        sub: u.public_id.clone(),
        role: u.role.clone(),
        tier: u.tier_code.clone(),
        iat: now.timestamp(),
        exp: exp.timestamp(),
    });
    (token, exp)
}

async fn open_session(conn: &mut crux_repo::MySqlConnection, ctx: &Ctx, u: &UserRow, client: &ClientInfo) -> AppResult<Session> {
    let now = ctx.clock.now();
    let refresh_token = ctx.ids.secret();
    let refresh_expires_at = now + rules::REFRESH_TOKEN_TTL;
    sessions::insert(conn, u.id, &rules::token_hash(&refresh_token), now, refresh_expires_at, client.user_agent.as_deref()).await?;
    let (access_token, access_expires_at) = issue_access(ctx, u);
    Ok(Session { access_token, access_expires_at, refresh_token, refresh_expires_at, user: UserView::from(u) })
}

fn verify_link(ctx: &Ctx, token: &str) -> String {
    format!("{}/verify?token={token}", ctx.config.app_base_url)
}

fn reset_link(ctx: &Ctx, token: &str) -> String {
    format!("{}/reset?token={token}", ctx.config.app_base_url)
}

async fn issue_verify_token(conn: &mut crux_repo::MySqlConnection, ctx: &Ctx, u: &UserRow) -> AppResult<()> {
    let now = ctx.clock.now();
    let token = ctx.ids.secret();
    auth_tokens::void_unused(conn, u.id, Purpose::VerifyEmail, now).await?;
    auth_tokens::insert(conn, u.id, Purpose::VerifyEmail, &rules::token_hash(&token), now, now + rules::VERIFY_EMAIL_TTL).await?;
    request_account_email(
        conn,
        ctx,
        &u.public_id,
        "auth.verify_email",
        &format!("auth.verify_email:{}", &rules::token_hash(&token)[..16]),
        json!({ "firstName": u.first_name, "link": verify_link(ctx, &token) }),
    )
    .await
}

/// `POST /auth/signup`: needs a valid invite; uses are taken in the same transaction.
pub async fn signup(ctx: &Ctx, input: SignupInput, client: &ClientInfo) -> AppResult<Session> {
    ratelimit::hit(ctx, &ratelimit::SIGNUP_PER_IP, &client.ip).await?;

    let email = rules::normalize_email(&input.email);
    let mut errors = ValidationErrors::new();
    errors.merge(rules::validate_email(&email));
    errors.merge(rules::validate_username(&input.username));
    errors.merge(rules::validate_name("firstName", &input.first_name));
    errors.merge(rules::validate_name("lastName", &input.last_name));
    errors.merge(rules::validate_time_zone(&input.time_zone));
    errors.merge(rules::validate_password(&input.password, &ctx.breached_passwords));
    errors.into_result()?;

    let hash = password::hash_blocking(input.password.clone()).await?;
    let now = ctx.clock.now();
    let mut tx = crux_repo::begin(&ctx.pool).await?;

    let code_hash = rules::token_hash(&rules::normalize_invite_code(&input.invite_code));
    let invite = invites::by_code_hash_for_update(&mut tx, &code_hash).await?.ok_or(AppError::InviteInvalid { reason: "unknown" })?;
    let state = rules::InviteState {
        email: invite.email.clone(),
        max_uses: invite.max_uses,
        uses: invite.uses,
        expires_at: invite.expires_at,
        revoked_at: invite.revoked_at,
    };
    rules::check_invite(&state, &email, now).map_err(|r| AppError::InviteInvalid { reason: invite_reason(r) })?;
    invites::increment_uses(&mut tx, invite.id).await?;

    let default_tier = tiers::default_tier(&mut tx).await?.ok_or_else(|| anyhow::anyhow!("no default tier"))?;
    let tier_id = signup_tier_id(invite.tier_id, default_tier.id);
    let tier = if tier_id == default_tier.id {
        default_tier
    } else {
        tiers::by_id(&mut tx, tier_id).await?.ok_or_else(|| anyhow::anyhow!("invite tier missing"))?
    };

    let public_id = ctx.ids.ulid();
    let user_id = users::insert(
        &mut tx,
        &NewUser {
            public_id: &public_id,
            username: input.username.trim(),
            email: &email,
            password_hash: &hash,
            first_name: input.first_name.trim(),
            last_name: input.last_name.trim(),
            time_zone: &input.time_zone,
            tier_id: tier.id,
        },
    )
    .await
    .map_err(duplicate_to_validation)?;
    settings::insert_defaults(&mut tx, user_id).await?;
    tiers::record_change(&mut tx, user_id, None, tier.id, None, "signup", now).await?;

    let user = users::by_id(&mut tx, user_id).await?.ok_or(AppError::NotFound)?;
    emit(&mut tx, ctx, topics::USER_CREATED, Some(&user.public_id), user_payload(&user, tier.storage_quota_bytes)).await?;
    issue_verify_token(&mut tx, ctx, &user).await?;
    let session = open_session(&mut tx, ctx, &user, client).await?;
    tx.commit().await?;
    Ok(session)
}

/// `POST /auth/login`. A wrong login name and a wrong password look and take the same.
pub async fn login(ctx: &Ctx, input: LoginInput, client: &ClientInfo) -> AppResult<Session> {
    let login = rules::normalize_email(&input.login);
    ratelimit::hit(ctx, &ratelimit::LOGIN_PER_IP, &client.ip).await?;
    ratelimit::hit(ctx, &ratelimit::LOGIN_PER_ACCOUNT, &login).await?;

    let mut conn = ctx.pool.acquire().await?;
    let user = users::by_login(&mut conn, &login).await?;
    let phc = user.as_ref().and_then(|u| u.password_hash.clone()).unwrap_or_else(|| password::dummy_hash().to_string());
    let ok = password::verify_blocking(input.password, phc).await;
    let user = match user {
        Some(u) if ok && u.password_hash.is_some() && UserStatus::from_db(&u.status).may_log_in() => u,
        _ => return Err(AppError::Unauthenticated),
    };
    ratelimit::reset(ctx, &ratelimit::LOGIN_PER_ACCOUNT, &login).await?;
    open_session(&mut conn, ctx, &user, client).await
}

/// `POST /auth/refresh`: rotates the refresh token. Reuse of a rotated token revokes everything.
pub async fn refresh(ctx: &Ctx, refresh_token: &str, client: &ClientInfo) -> AppResult<Session> {
    let now = ctx.clock.now();
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let session = sessions::by_token_hash_for_update(&mut tx, &rules::token_hash(refresh_token)).await?.ok_or(AppError::Unauthenticated)?;
    let state = rules::SessionState { expires_at: session.expires_at, revoked_at: session.revoked_at };
    match rules::decide_refresh(&state, now) {
        RefreshDecision::Expired => Err(AppError::Unauthenticated),
        RefreshDecision::Reused => {
            let n = sessions::revoke_all_for_user(&mut tx, session.user_id, now).await?;
            tx.commit().await?;
            tracing::warn!(revoked = n, "refresh token reuse detected; all sessions revoked");
            Err(AppError::Unauthenticated)
        }
        RefreshDecision::Rotate => {
            sessions::revoke(&mut tx, session.id, now).await?;
            let user = users::by_id(&mut tx, session.user_id).await?.ok_or(AppError::Unauthenticated)?;
            if !UserStatus::from_db(&user.status).may_log_in() {
                return Err(AppError::Unauthenticated);
            }
            let out = open_session(&mut tx, ctx, &user, client).await?;
            tx.commit().await?;
            Ok(out)
        }
    }
}

/// `POST /auth/logout`: revokes the session behind the cookie. Idempotent.
pub async fn logout(ctx: &Ctx, refresh_token: Option<&str>) -> AppResult<()> {
    let Some(token) = refresh_token else { return Ok(()) };
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    if let Some(s) = sessions::by_token_hash_for_update(&mut tx, &rules::token_hash(token)).await? {
        if s.revoked_at.is_none() {
            sessions::revoke(&mut tx, s.id, ctx.clock.now()).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Resolves a bearer access token to the acting user. Disabled users are signed out at once
/// rather than at token expiry.
pub async fn actor_from_token(ctx: &Ctx, token: &str) -> AppResult<Actor> {
    let claims = ctx.keys.verify_access(token, ctx.clock.now().timestamp()).map_err(|_| AppError::Unauthenticated)?;
    let mut conn = ctx.pool.acquire().await?;
    let u = users::by_public_id(&mut conn, &claims.sub).await?.ok_or(AppError::Unauthenticated)?;
    if !UserStatus::from_db(&u.status).may_log_in() {
        return Err(AppError::Unauthenticated);
    }
    Ok(Actor { user_id: u.id, public_id: u.public_id, role: Role::from_db(&u.role), email_verified: u.email_verified_at.is_some() })
}

/// `POST /auth/verify-email`: sends a fresh link. No-op once verified.
pub async fn request_verify_email(ctx: &Ctx, actor: &Actor) -> AppResult<()> {
    if actor.email_verified {
        return Ok(());
    }
    ratelimit::hit(ctx, &ratelimit::ACCOUNT_EMAIL_PER_USER, &actor.public_id).await?;
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let u = users::by_id(&mut tx, actor.user_id).await?.ok_or(AppError::NotFound)?;
    issue_verify_token(&mut tx, ctx, &u).await?;
    tx.commit().await?;
    Ok(())
}

/// `POST /auth/verify-email/confirm {token}`.
pub async fn confirm_verify_email(ctx: &Ctx, token: &str) -> AppResult<UserView> {
    let now = ctx.clock.now();
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let row = auth_tokens::by_hash_for_update(&mut tx, Purpose::VerifyEmail, &rules::token_hash(token)).await?;
    let row = row
        .filter(|r| rules::one_time_token_usable(&rules::OneTimeTokenState { expires_at: r.expires_at, used_at: r.used_at }, now))
        .ok_or_else(|| AppError::Validation(ValidationErrors::single("token", "invalid", "This link is invalid or has expired.")))?;
    auth_tokens::mark_used(&mut tx, row.id, now).await?;
    users::set_email_verified(&mut tx, row.user_id, now).await?;
    let u = users::by_id(&mut tx, row.user_id).await?.ok_or(AppError::NotFound)?;
    let tier = tiers::by_id(&mut tx, u.tier_id).await?.ok_or(AppError::NotFound)?;
    emit(&mut tx, ctx, topics::USER_UPDATED, Some(&u.public_id), user_payload(&u, tier.storage_quota_bytes)).await?;
    tx.commit().await?;
    Ok(UserView::from(&u))
}

/// `POST /auth/password/forgot {email}`: always succeeds, so it can't be used to probe accounts.
pub async fn forgot_password(ctx: &Ctx, email: &str, client: &ClientInfo) -> AppResult<()> {
    let email = rules::normalize_email(email);
    ratelimit::hit(ctx, &ratelimit::LOGIN_PER_IP, &client.ip).await?;
    if ratelimit::hit(ctx, &ratelimit::ACCOUNT_EMAIL_PER_USER, &email).await.is_err() {
        return Ok(());
    }
    let now = ctx.clock.now();
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let Some(u) = users::by_email(&mut tx, &email).await? else { return Ok(()) };
    if !UserStatus::from_db(&u.status).may_log_in() {
        return Ok(());
    }
    let token = ctx.ids.secret();
    auth_tokens::void_unused(&mut tx, u.id, Purpose::ResetPassword, now).await?;
    auth_tokens::insert(&mut tx, u.id, Purpose::ResetPassword, &rules::token_hash(&token), now, now + rules::RESET_PASSWORD_TTL).await?;
    request_account_email(
        &mut tx,
        ctx,
        &u.public_id,
        "auth.password_reset",
        &format!("auth.password_reset:{}", &rules::token_hash(&token)[..16]),
        json!({ "firstName": u.first_name, "link": reset_link(ctx, &token) }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// `POST /auth/password/reset {token, password}`: sets the password and signs out every session.
pub async fn reset_password(ctx: &Ctx, token: &str, new_password: &str) -> AppResult<()> {
    rules::validate_password(new_password, &ctx.breached_passwords).into_result()?;
    let hash = password::hash_blocking(new_password.to_string()).await?;
    let now = ctx.clock.now();
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let row = auth_tokens::by_hash_for_update(&mut tx, Purpose::ResetPassword, &rules::token_hash(token)).await?;
    let row = row
        .filter(|r| rules::one_time_token_usable(&rules::OneTimeTokenState { expires_at: r.expires_at, used_at: r.used_at }, now))
        .ok_or_else(|| AppError::Validation(ValidationErrors::single("token", "invalid", "This link is invalid or has expired.")))?;
    auth_tokens::mark_used(&mut tx, row.id, now).await?;
    users::set_password_hash(&mut tx, row.user_id, &hash).await?;
    sessions::revoke_all_for_user(&mut tx, row.user_id, now).await?;
    tx.commit().await?;
    Ok(())
}

/// `POST /internal/v1/service-token`: other services trade the shared bootstrap secret for a
/// 5-minute token (spec 3.2; OPEN-QUESTIONS.md).
pub fn issue_service_token(ctx: &Ctx, presented_secret: &str, caller: &str, audience: &str) -> AppResult<String> {
    const SERVICES: &[&str] = &["core", "files", "ai", "notify"];
    let secret_ok = constant_time_eq(presented_secret.as_bytes(), ctx.config.service_secret.as_bytes());
    if !secret_ok || !SERVICES.contains(&caller) || !SERVICES.contains(&audience) {
        return Err(AppError::Unauthenticated);
    }
    Ok(ctx.keys.sign_service(caller, audience, ctx.clock.now().timestamp()))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }

    #[test]
    fn duplicate_keys_become_field_errors() {
        let AppError::Validation(e) = duplicate_to_validation(RepoError::Duplicate("uq_user_username".into())) else { panic!() };
        assert_eq!(e.0[0].field, "username");
        let AppError::Validation(e) = duplicate_to_validation(RepoError::Duplicate("uq_user_email".into())) else { panic!() };
        assert_eq!(e.0[0].field, "email");
        assert!(matches!(duplicate_to_validation(RepoError::Duplicate("other".into())), AppError::Conflict { .. }));
    }
}
