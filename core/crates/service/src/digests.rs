//! Daily notifications core requests (backend-spec 8): `plan.today` at 07:30 local and
//! `flash.due` at 08:00 local (only with cards due). Part of deadlines.tick; notify decides whether
//! the user wants them.

use chrono::{DateTime, Duration, NaiveTime, Utc};
use chrono_tz::Tz;
use serde_json::json;

use crate::time::iso;
use crate::{AppResult, Ctx};

/// After this local hour a missed morning digest is no longer sent (the job was down).
const LATEST_HOUR: u32 = 12;

fn local(tz: Tz, now: DateTime<Utc>, h: u32, m: u32) -> DateTime<Utc> {
    let d = now.with_timezone(&tz).date_naive();
    crux_domain::localtime::to_utc(tz, d.and_time(NaiveTime::from_hms_opt(h, m, 0).expect("time")))
}

/// Each (kind, user, local date) is requested once: a Redis marker, and notify's dedupe key as well.
async fn once(ctx: &Ctx, kind: &str, user_ref: &str, date: &str) -> AppResult<bool> {
    Ok(ctx
        .kv
        .set_nx(&format!("crux:digest:{kind}:{user_ref}:{date}"), "1", std::time::Duration::from_secs(48 * 3600))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?)
}

pub async fn tick(ctx: &Ctx) -> AppResult<u64> {
    let now = ctx.clock.now();
    let mut n = 0;
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    for (user_id, user_ref, tz) in crux_repo::study::users_with_active_courses(&mut tx).await? {
        let tz: Tz = tz.parse().unwrap_or(chrono_tz::Europe::Amsterdam);
        let date = now.with_timezone(&tz).date_naive().to_string();
        let noon = local(tz, now, LATEST_HOUR, 0);

        let plan_at = local(tz, now, 7, 30);
        if now >= plan_at && now < noon && once(ctx, "plan.today", &user_ref, &date).await? {
            let day_start = crux_domain::fsrs::local_day_start(now, tz);
            let items: Vec<_> = crux_repo::plan::between(&mut tx, user_id, day_start, day_start + Duration::days(1))
                .await?
                .into_iter()
                .filter(|e| e.status == "Planned")
                .take(10)
                .map(|e| json!({ "description": e.description, "startAt": iso(e.start_at), "courseCode": e.course_code, "kind": e.kind }))
                .collect();
            if !items.is_empty() {
                request(
                    &mut tx,
                    ctx,
                    &user_ref,
                    "plan.today",
                    &format!("plan.today:{date}"),
                    plan_at,
                    json!({ "date": date, "items": items }),
                )
                .await?;
                n += 1;
            }
        }

        let flash_at = local(tz, now, 8, 0);
        if now >= flash_at && now < noon && once(ctx, "flash.due", &user_ref, &date).await? {
            let mut due = 0;
            for c in crux_repo::flash::active_course_ids(&mut tx, user_id).await? {
                due += crux_repo::study::due_cards(&mut tx, user_id, c, now).await?;
            }
            if due > 0 {
                request(
                    &mut tx,
                    ctx,
                    &user_ref,
                    "flash.due",
                    &format!("flash.due:{date}"),
                    flash_at,
                    json!({ "date": date, "dueCount": due }),
                )
                .await?;
                n += 1;
            }
        }
    }
    tx.commit().await?;
    Ok(n)
}

async fn request(
    conn: &mut crux_repo::MySqlConnection,
    ctx: &Ctx,
    user_ref: &str,
    kind: &str,
    dedupe: &str,
    due: DateTime<Utc>,
    payload: serde_json::Value,
) -> AppResult<()> {
    crate::events::emit(
        conn,
        ctx,
        crux_domain::topics::NOTIFY_REQUESTED,
        Some(user_ref),
        json!({ "userRef": user_ref, "kind": kind, "dedupeKey": dedupe, "dueAt": iso(due), "payload": payload }),
    )
    .await
}
