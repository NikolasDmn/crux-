//! Publishing = writing an Outbox row in the caller's transaction (spec 3.3). Never publish
//! directly; the relay job does that.

use crux_repo::messaging::{self, NewOutbox};
use crux_repo::users::UserRow;
use crux_repo::MySqlConnection;
use serde_json::json;

use crate::contracts;
use crate::{AppResult, Ctx};

pub async fn emit(
    conn: &mut MySqlConnection,
    ctx: &Ctx,
    topic: &str,
    aggregate_ref: Option<&str>,
    payload: serde_json::Value,
) -> AppResult<()> {
    contracts::validate(topic, 1, &payload).map_err(|e| anyhow::anyhow!(e))?;
    let event_id = ctx.ids.ulid();
    messaging::insert_outbox(
        conn,
        &NewOutbox {
            event_id: &event_id,
            topic,
            schema_version: 1,
            aggregate_ref,
            payload: &payload,
            created_at: ctx.clock.now(),
        },
    )
    .await?;
    Ok(())
}

/// The `user.*` payload (spec 3.3 catalogue). `quota` comes from the user's tier row.
pub fn user_payload(u: &UserRow, storage_quota_bytes: i64) -> serde_json::Value {
    json!({
        "userRef": u.public_id,
        "email": u.email,
        "emailVerified": u.email_verified_at.is_some(),
        "timeZone": u.time_zone,
        "locale": u.locale,
        "tierCode": u.tier_code,
        "storageQuotaBytes": storage_quota_bytes,
    })
}

/// `notify.requested` for account emails (verify, reset). See OPEN-QUESTIONS.md on `transactional`.
pub async fn request_account_email(
    conn: &mut MySqlConnection,
    ctx: &Ctx,
    user_ref: &str,
    kind: &str,
    dedupe_key: &str,
    payload: serde_json::Value,
) -> AppResult<()> {
    let body = json!({
        "userRef": user_ref,
        "kind": kind,
        "dedupeKey": dedupe_key,
        "dueAt": crate::time::iso(ctx.clock.now()),
        "transactional": true,
        "payload": payload,
    });
    emit(conn, ctx, crux_domain::topics::NOTIFY_REQUESTED, Some(user_ref), body).await
}
