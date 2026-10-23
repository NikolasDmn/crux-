//! purge.tokens (daily): expired `AuthToken` / `UserSession` rows.

use chrono::Duration;
use crux_clients::clock::Clock;
use crux_repo::{auth_tokens, sessions, MySqlPool};

/// Rotated sessions are kept this long so a replayed old refresh token is still recognised as
/// reuse (and revokes everything) rather than just "unknown".
pub const KEEP_REVOKED: Duration = Duration::days(30);

pub async fn purge(pool: &MySqlPool, clock: &dyn Clock) -> anyhow::Result<u64> {
    let now = clock.now();
    let mut conn = pool.acquire().await?;
    let s = sessions::purge(&mut conn, now, now - KEEP_REVOKED).await?;
    let t = auth_tokens::purge(&mut conn, now).await?;
    Ok(s + t)
}
