//! SSE tickets (spec 3.4): EventSource can't send headers, so the client trades its access token
//! for a 60-second single-use ticket, passed as `?ticket=`.
//!
//! Shared contract with the Python services, which redeem tickets for their own streams:
//! Redis key `crux:sse:<sha256(ticket)>`, value `{"userRef": "...", "role": "..."}`, read with GETDEL.

use std::time::Duration;

use crux_domain::auth::{token_hash, SSE_TICKET_TTL_SECS};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::time::iso;
use crate::{Actor, AppError, AppResult, Ctx};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Ticket {
    pub ticket: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketHolder {
    pub user_ref: String,
    pub role: String,
}

pub fn ticket_key(ticket: &str) -> String {
    format!("crux:sse:{}", token_hash(ticket))
}

pub async fn issue(ctx: &Ctx, actor: &Actor) -> AppResult<Ticket> {
    let ticket = ctx.ids.secret();
    let holder = TicketHolder { user_ref: actor.public_id.clone(), role: actor.role.as_db().to_string() };
    let value = serde_json::to_string(&holder).map_err(anyhow::Error::from)?;
    ctx.kv.set(&ticket_key(&ticket), &value, Duration::from_secs(SSE_TICKET_TTL_SECS)).await?;
    let expires_at = ctx.clock.now() + chrono::Duration::seconds(SSE_TICKET_TTL_SECS as i64);
    Ok(Ticket { ticket, expires_at: iso(expires_at) })
}

/// Single use: the second redeem of the same ticket fails.
pub async fn redeem(ctx: &Ctx, ticket: &str) -> AppResult<TicketHolder> {
    let raw = ctx.kv.take(&ticket_key(ticket)).await?.ok_or(AppError::Unauthenticated)?;
    serde_json::from_str(&raw).map_err(|_| AppError::Unauthenticated)
}
