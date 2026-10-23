//! Fixed-window limits on Redis counters (spec 12). Keys never contain raw emails: they're hashed.

use std::time::Duration;

use crux_domain::auth::token_hash;

use crate::{AppError, AppResult, Ctx};

#[derive(Debug, Clone, Copy)]
pub struct Limit {
    pub name: &'static str,
    pub max: u64,
    pub window: Duration,
}

pub const LOGIN_PER_ACCOUNT: Limit = Limit { name: "login.account", max: 5, window: Duration::from_secs(15 * 60) };
pub const LOGIN_PER_IP: Limit = Limit { name: "login.ip", max: 20, window: Duration::from_secs(15 * 60) };
pub const SIGNUP_PER_IP: Limit = Limit { name: "signup.ip", max: 5, window: Duration::from_secs(60 * 60) };
/// Not in the spec; stops a signed-in user from mail-bombing themselves or someone else.
pub const ACCOUNT_EMAIL_PER_USER: Limit = Limit { name: "account_email", max: 5, window: Duration::from_secs(60 * 60) };

fn key(limit: &Limit, subject: &str) -> String {
    format!("crux:rl:{}:{}", limit.name, &token_hash(subject)[..32])
}

/// Counts this hit; `RateLimited` once the window is full.
pub async fn hit(ctx: &Ctx, limit: &Limit, subject: &str) -> AppResult<()> {
    let n = ctx.kv.incr_window(&key(limit, subject), limit.window).await?;
    if n > limit.max {
        return Err(AppError::RateLimited { retry_after_secs: limit.window.as_secs() });
    }
    Ok(())
}

pub async fn reset(ctx: &Ctx, limit: &Limit, subject: &str) -> AppResult<()> {
    ctx.kv.del(&key(limit, subject)).await?;
    Ok(())
}
