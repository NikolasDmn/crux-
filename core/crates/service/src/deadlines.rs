//! Exams and deadlines (backend-spec 5.10): CRUD on the user's subscription, the exam layout
//! (`DeadlineComponent`), recording a result, and the reminder tick.

use chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use crux_domain::study;
use crux_domain::ValidationErrors;
use crux_repo::deadlines::{self as repo, DeadlineRow, DeadlineWrite, LayoutRow};
use crux_repo::MySqlConnection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;

use crate::courses::readable;
use crate::time::iso;
use crate::{Actor, AppError, AppResult, Ctx};

const TYPES: [&str; 3] = ["Exam", "Resit", "Deadline"];
const STATUSES: [&str; 4] = ["Upcoming", "Done", "Missed", "Cancelled"];

/// The actor's deadline on this course, by PublicId (deadlines hang off the user's subscription).
pub async fn deadline_id_for(conn: &mut MySqlConnection, user_id: i64, course_id: i64, deadline_ref: &str) -> AppResult<Option<i64>> {
    Ok(crux_repo::mocks::deadline_id_for(conn, user_id, course_id, deadline_ref).await?)
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LayoutItem {
    pub component_id: String,
    pub question_count: Option<i32>,
    pub open_count: Option<i32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeadlineView {
    pub id: String,
    pub course_id: String,
    /// Exam, Resit, Deadline.
    #[schema(value_type = String)]
    pub r#type: String,
    pub title: Option<String>,
    pub resit_of_id: Option<String>,
    pub starts_at: String,
    pub duration_min: Option<i32>,
    pub format: Option<String>,
    pub pass_mark: Option<f64>,
    /// Upcoming, Done, Missed, Cancelled.
    pub status: String,
    pub grade_value: Option<f64>,
    pub grade_text: Option<String>,
    pub grade_provisional: bool,
    pub check_in_note: Option<String>,
    /// From the grade and the pass mark (or the course's grade scale).
    pub passed: Option<bool>,
    /// The exam has started and has no result yet: ask "how did it go?" (spec 5.10).
    pub awaiting_result: bool,
    /// Failed with no resit planned yet: offer one.
    pub offer_resit: bool,
    pub layout: Vec<LayoutItem>,
    pub updated_at: String,
    pub version: i32,
}

struct Scale {
    kind: String,
    pass_value: Option<f64>,
    labels: Vec<(String, bool)>,
}

async fn scale(conn: &mut MySqlConnection, course_id: i64) -> AppResult<Scale> {
    let (kind, pass_value) = repo::scale_of_course(conn, course_id).await?.unwrap_or(("Numeric".into(), None));
    let labels = repo::scale_labels(conn, course_id)
        .await?
        .and_then(|j| j.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|l| Some((l["label"].as_str()?.to_string(), l["pass"].as_bool().unwrap_or(false))))
        .collect();
    Ok(Scale { kind, pass_value, labels })
}

fn passed_of(d: &DeadlineRow, s: &Scale) -> Option<bool> {
    if d.status != "Done" {
        return None;
    }
    if s.kind == "Letter" {
        let text = d.grade_text.as_deref()?.trim();
        return s.labels.iter().find(|(l, _)| l.eq_ignore_ascii_case(text)).map(|(_, p)| *p);
    }
    study::passed(d.grade_value, d.pass_mark, s.pass_value, if s.kind == "PassFail" { d.grade_text.as_deref() } else { None })
}

async fn views(conn: &mut MySqlConnection, rows: &[DeadlineRow], now: DateTime<Utc>) -> AppResult<Vec<DeadlineView>> {
    let layouts: Vec<LayoutRow> = repo::layouts(conn, &rows.iter().map(|d| d.id).collect::<Vec<_>>()).await?;
    let mut out = Vec::with_capacity(rows.len());
    for d in rows {
        let s = scale(conn, d.course_id).await?;
        let passed = passed_of(d, &s);
        let exam = matches!(d.dtype.as_str(), "Exam" | "Resit");
        let has_resit = rows.iter().any(|o| o.resit_of_ref.is_some() && o.resit_of_ref == d.public_id);
        out.push(DeadlineView {
            id: d.public_id.clone().unwrap_or_default(),
            course_id: d.course_ref.clone(),
            r#type: d.dtype.clone(),
            title: d.title.clone(),
            resit_of_id: d.resit_of_ref.clone(),
            starts_at: iso(d.starts_at),
            duration_min: d.duration_min,
            format: d.format.clone(),
            pass_mark: d.pass_mark,
            status: d.status.clone(),
            grade_value: d.grade_value,
            grade_text: d.grade_text.clone(),
            grade_provisional: d.grade_provisional,
            check_in_note: d.check_in_note.clone(),
            passed,
            awaiting_result: exam && d.status == "Upcoming" && d.starts_at <= now,
            offer_resit: exam && passed == Some(false) && !has_resit,
            layout: layouts
                .iter()
                .filter(|l| l.deadline_id == d.id)
                .map(|l| LayoutItem { component_id: l.component_ref.clone(), question_count: l.question_count, open_count: l.open_count })
                .collect(),
            updated_at: iso(d.updated_at),
            version: d.row_version,
        });
    }
    Ok(out)
}

/// The course the actor reads and is subscribed to, with the subscription (id, status).
async fn subscribed(
    conn: &mut MySqlConnection,
    actor: &Actor,
    course_ref: &str,
) -> AppResult<(crux_repo::courses::CourseRow, i64, String)> {
    let course = readable(conn, actor, course_ref).await?;
    let (sub, status) = repo::subscription_id(conn, actor.user_id, course.id).await?.ok_or(AppError::NotFound)?;
    Ok((course, sub, status))
}

pub async fn list(ctx: &Ctx, actor: &Actor, course_ref: &str) -> AppResult<Vec<DeadlineView>> {
    let mut conn = ctx.pool.acquire().await?;
    let (course, _, _) = subscribed(&mut conn, actor, course_ref).await?;
    let rows = repo::for_course(&mut conn, actor.user_id, course.id).await?;
    views(&mut conn, &rows, ctx.clock.now()).await
}

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeadlineInput {
    /// Exam, Resit, Deadline (default).
    #[serde(rename = "type")]
    #[schema(rename = "type")]
    pub dtype: Option<String>,
    pub title: Option<String>,
    /// A resit points at the exam it repeats.
    pub resit_of_id: Option<String>,
    pub starts_at: Option<DateTime<Utc>>,
    pub duration_min: Option<i32>,
    pub format: Option<String>,
    pub pass_mark: Option<f64>,
    /// The check-in result: Done (with a grade), Missed, Cancelled.
    pub status: Option<String>,
    pub grade_value: Option<f64>,
    pub grade_text: Option<String>,
    pub grade_provisional: Option<bool>,
    pub check_in_note: Option<String>,
}

struct Merged {
    dtype: String,
    title: Option<String>,
    resit_of: Option<i64>,
    starts_at: DateTime<Utc>,
    duration_min: Option<i32>,
    format: Option<String>,
    pass_mark: Option<f64>,
    status: String,
    grade_value: Option<f64>,
    grade_text: Option<String>,
    grade_provisional: bool,
    check_in_note: Option<String>,
}

impl Merged {
    fn write(&self) -> DeadlineWrite<'_> {
        DeadlineWrite {
            dtype: &self.dtype,
            title: self.title.as_deref(),
            resit_of: self.resit_of,
            starts_at: self.starts_at,
            duration_min: self.duration_min,
            format: self.format.as_deref(),
            pass_mark: self.pass_mark,
            status: &self.status,
            grade_value: self.grade_value,
            grade_text: self.grade_text.as_deref(),
            grade_provisional: self.grade_provisional,
            check_in_note: self.check_in_note.as_deref(),
        }
    }

    fn check(&self) -> ValidationErrors {
        let mut e = ValidationErrors::new();
        if !TYPES.contains(&self.dtype.as_str()) {
            e.push("type", "invalid", "Use Exam, Resit or Deadline.");
        }
        if !STATUSES.contains(&self.status.as_str()) {
            e.push("status", "invalid", "Use Upcoming, Done, Missed or Cancelled.");
        }
        if self.resit_of.is_some() && self.dtype != "Resit" {
            e.push("resitOfId", "resit_only", "Only a resit points at an earlier exam.");
        }
        if self.duration_min.is_some_and(|d| !(1..=24 * 60).contains(&d)) {
            e.push("durationMin", "range", "Between 1 minute and 24 hours.");
        }
        if (self.grade_value.is_some() || self.grade_text.is_some()) && self.status != "Done" {
            e.push("gradeValue", "needs_done", "A grade comes with status Done.");
        }
        if self.grade_value.is_some_and(|g| !g.is_finite()) {
            e.push("gradeValue", "invalid", "Enter a number.");
        }
        if self.title.as_ref().is_some_and(|t| t.len() > 255) || self.format.as_ref().is_some_and(|t| t.len() > 255) {
            e.push("title", "too_long", "Up to 255 characters.");
        }
        if self.grade_text.as_ref().is_some_and(|t| t.len() > 16) {
            e.push("gradeText", "too_long", "Up to 16 characters.");
        }
        e
    }
}

async fn resit_target(conn: &mut MySqlConnection, actor: &Actor, course_id: i64, r: Option<&str>) -> AppResult<Option<i64>> {
    match r {
        Some(r) => Ok(Some(
            repo::by_public(conn, actor.user_id, r)
                .await?
                .filter(|d| d.course_id == course_id && matches!(d.dtype.as_str(), "Exam" | "Resit"))
                .ok_or_else(|| ValidationErrors::single("resitOfId", "unknown", "Pick an exam of this course."))?
                .id,
        )),
        None => Ok(None),
    }
}

pub async fn create(ctx: &Ctx, actor: &Actor, course_ref: &str, input: DeadlineInput) -> AppResult<DeadlineView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let (course, sub, _) = subscribed(&mut tx, actor, course_ref).await?;
    let starts_at = input.starts_at.ok_or_else(|| ValidationErrors::single("startsAt", "required", "When is it?"))?;
    let m = Merged {
        dtype: input.dtype.unwrap_or_else(|| "Deadline".into()),
        title: input.title,
        resit_of: resit_target(&mut tx, actor, course.id, input.resit_of_id.as_deref()).await?,
        starts_at,
        duration_min: input.duration_min,
        format: input.format,
        pass_mark: input.pass_mark,
        status: input.status.unwrap_or_else(|| "Upcoming".into()),
        grade_value: input.grade_value,
        grade_text: input.grade_text,
        grade_provisional: input.grade_provisional.unwrap_or(false),
        check_in_note: input.check_in_note,
    };
    m.check().into_result()?;
    let public_id = ctx.ids.ulid();
    let id = repo::insert(&mut tx, &public_id, sub, &m.write()).await?;
    let row = repo::by_id(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    after_change(&mut tx, ctx, actor, &row).await?;
    let view = views(&mut tx, &[row], ctx.clock.now()).await?.remove(0);
    tx.commit().await?;
    Ok(view)
}

pub async fn patch(ctx: &Ctx, actor: &Actor, deadline_ref: &str, version: i32, input: DeadlineInput) -> AppResult<DeadlineView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let d = repo::by_public(&mut tx, actor.user_id, deadline_ref).await?.ok_or(AppError::NotFound)?;
    let resit_of = match &input.resit_of_id {
        Some(r) if r.is_empty() => None,
        Some(r) => resit_target(&mut tx, actor, d.course_id, Some(r)).await?,
        None => match &d.resit_of_ref {
            Some(r) => resit_target(&mut tx, actor, d.course_id, Some(r)).await?,
            None => None,
        },
    };
    let status = input.status.clone().unwrap_or_else(|| d.status.clone());
    let clears_grade = status != "Done";
    let m = Merged {
        dtype: input.dtype.unwrap_or_else(|| d.dtype.clone()),
        title: input.title.or_else(|| d.title.clone()),
        resit_of,
        starts_at: input.starts_at.unwrap_or(d.starts_at),
        duration_min: input.duration_min.or(d.duration_min),
        format: input.format.or_else(|| d.format.clone()),
        pass_mark: input.pass_mark.or(d.pass_mark),
        // Moving back out of Done drops the grade (schema CHECK).
        grade_value: if clears_grade && input.grade_value.is_none() { None } else { input.grade_value.or(d.grade_value) },
        grade_text: if clears_grade && input.grade_text.is_none() { None } else { input.grade_text.or_else(|| d.grade_text.clone()) },
        status,
        grade_provisional: input.grade_provisional.unwrap_or(d.grade_provisional),
        check_in_note: input.check_in_note.or_else(|| d.check_in_note.clone()),
    };
    m.check().into_result()?;
    if d.row_version != version {
        let current = views(&mut tx, std::slice::from_ref(&d), ctx.clock.now()).await?.remove(0);
        return Err(AppError::Conflict {
            message: "This deadline changed somewhere else.".into(),
            current: Some(serde_json::to_value(current).map_err(anyhow::Error::from)?),
        });
    }
    repo::update(&mut tx, d.id, version, &m.write()).await?;
    let row = repo::by_id(&mut tx, d.id).await?.ok_or(AppError::NotFound)?;
    reminders_after_edit(&mut tx, ctx, actor, &d, &row).await?;
    if row.status == "Done" && d.status != "Done" || row.grade_value != d.grade_value || row.grade_text != d.grade_text {
        record_result(&mut tx, actor, &row).await?;
    }
    after_change(&mut tx, ctx, actor, &row).await?;
    let view = views(&mut tx, &[row], ctx.clock.now()).await?.remove(0);
    tx.commit().await?;
    Ok(view)
}

/// Spec 5.10 pass: subscription Passed with the final grade; the course is archived for the user
/// (cards suspended, out of the plan and warm-ups). A fail keeps the subscription Active.
async fn record_result(conn: &mut MySqlConnection, actor: &Actor, d: &DeadlineRow) -> AppResult<()> {
    if !matches!(d.dtype.as_str(), "Exam" | "Resit") || d.status != "Done" {
        return Ok(());
    }
    let s = scale(conn, d.course_id).await?;
    let passed = passed_of(d, &s);
    match passed {
        Some(true) => {
            repo::set_final_grade(conn, d.subscription_id, "Passed", d.grade_value, d.grade_text.as_deref(), Some(true)).await?;
            crate::flash::on_subscription_status(conn, actor.user_id, d.course_id, true).await?;
        }
        Some(false) => {
            repo::set_final_grade(conn, d.subscription_id, "Active", d.grade_value, d.grade_text.as_deref(), Some(false)).await?;
        }
        None => {}
    }
    Ok(())
}

/// Spec 8 "a changed deadline sends a new request; the old one is cancelled by key": a moved exam
/// re-requests the reminders already requested (notify moves the pending ones); an exam that is no
/// longer Upcoming cancels them.
async fn reminders_after_edit(
    conn: &mut MySqlConnection,
    ctx: &Ctx,
    actor: &Actor,
    before: &DeadlineRow,
    after: &DeadlineRow,
) -> AppResult<()> {
    let dref = after.public_id.clone().unwrap_or_default();
    let keys = [("exam.eve", format!("exam.eve:{dref}")), ("exam.checkin", format!("exam.checkin:{dref}"))];
    if after.status != "Upcoming" && before.status == "Upcoming" {
        for (kind, key) in &keys {
            crate::events::emit(
                conn,
                ctx,
                crux_domain::topics::NOTIFY_REQUESTED,
                Some(&actor.public_id),
                json!({ "userRef": actor.public_id, "kind": kind, "dedupeKey": key, "dueAt": iso(ctx.clock.now()), "payload": {}, "cancel": true }),
            )
            .await?;
        }
        return Ok(());
    }
    if after.starts_at != before.starts_at && after.status == "Upcoming" {
        let tz: Tz = crux_repo::users::by_id(conn, actor.user_id)
            .await?
            .and_then(|u| u.time_zone.parse().ok())
            .unwrap_or(chrono_tz::Europe::Amsterdam);
        let (eve, check_in) = reminder_times(after.starts_at, tz);
        let payload = reminder_payload(after);
        if before.eve_requested_at.is_some() {
            notify(conn, ctx, &actor.public_id, "exam.eve", &keys[0].1, eve, payload.clone()).await?;
        }
        if before.check_in_requested_at.is_some() {
            notify(conn, ctx, &actor.public_id, "exam.checkin", &keys[1].1, check_in, payload).await?;
        }
    }
    Ok(())
}

fn reminder_payload(d: &DeadlineRow) -> serde_json::Value {
    json!({
        "deadlineId": d.public_id.clone().unwrap_or_default(), "courseId": d.course_ref, "courseCode": d.course_code,
        "title": d.title, "type": d.dtype, "startsAt": iso(d.starts_at), "durationMin": d.duration_min, "format": d.format,
    })
}

/// Plan rebuild on any deadline change (spec 5.8).
async fn after_change(conn: &mut MySqlConnection, ctx: &Ctx, actor: &Actor, _d: &DeadlineRow) -> AppResult<()> {
    crate::plan::rebuild_in(conn, ctx, actor.user_id).await?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LayoutInputItem {
    pub component_id: String,
    pub question_count: Option<i32>,
    pub open_count: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct LayoutInput {
    pub components: Vec<LayoutInputItem>,
}

/// The exam's layout: which lectures, how many questions each (the mock builder uses it).
pub async fn set_layout(ctx: &Ctx, actor: &Actor, deadline_ref: &str, input: LayoutInput) -> AppResult<DeadlineView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let d = repo::by_public(&mut tx, actor.user_id, deadline_ref).await?.ok_or(AppError::NotFound)?;
    let mut e = ValidationErrors::new();
    let refs: Vec<String> = input.components.iter().map(|c| c.component_id.clone()).collect();
    let ids = crux_repo::catalog::component_ids_by_public(&mut tx, d.course_id, &refs).await?;
    let mut rows = Vec::new();
    for (i, c) in input.components.iter().enumerate() {
        let Some((id, _)) = ids.iter().find(|(_, r)| *r == c.component_id) else {
            e.push(&format!("components[{i}].componentId"), "unknown", "Pick a lecture of this course.");
            continue;
        };
        if rows.iter().any(|(r, _, _)| r == id) {
            e.push(&format!("components[{i}].componentId"), "duplicate", "Each lecture once.");
        }
        if c.question_count.is_some_and(|n| n < 0) || c.open_count.is_some_and(|n| n < 0) {
            e.push(&format!("components[{i}].questionCount"), "range", "Counts can't be negative.");
        }
        if let (Some(q), Some(o)) = (c.question_count, c.open_count) {
            if o > q {
                e.push(&format!("components[{i}].openCount"), "range", "Open questions are part of the question count.");
            }
        }
        rows.push((*id, c.question_count, c.open_count));
    }
    e.into_result()?;
    repo::set_layout(&mut tx, d.id, &rows).await?;
    let row = repo::by_id(&mut tx, d.id).await?.ok_or(AppError::NotFound)?;
    let view = views(&mut tx, &[row], ctx.clock.now()).await?.remove(0);
    tx.commit().await?;
    Ok(view)
}

/// Exams that have started and have no result yet (for /next and the inbox).
pub async fn awaiting_result(conn: &mut MySqlConnection, user_id: i64, now: DateTime<Utc>) -> AppResult<Vec<DeadlineView>> {
    let rows = repo::awaiting_result(conn, user_id, now).await?;
    views(conn, &rows, now).await
}

// ---------- deadlines.tick ----------

fn local_at(tz: Tz, t: DateTime<Utc>, day_offset: i64, h: u32) -> DateTime<Utc> {
    let d = t.with_timezone(&tz).date_naive() + Duration::days(day_offset);
    tz.from_local_datetime(&d.and_time(NaiveTime::from_hms_opt(h, 0, 0).expect("hour")))
        .earliest()
        .map(|x| x.with_timezone(&Utc))
        .unwrap_or(t)
}

/// Eve reminder at 18:00 local the day before; check-in at 17:00 local on exam day (spec 5.10).
/// Only computes the times; notify decides whether the kind is enabled (milestone 6).
pub fn reminder_times(starts_at: DateTime<Utc>, tz: Tz) -> (DateTime<Utc>, DateTime<Utc>) {
    (local_at(tz, starts_at, -1, 18), local_at(tz, starts_at, 0, 17))
}

/// Every 15 min (spec 10): exam eve and check-in `notify.requested`, and the behind-plan check.
pub async fn tick(ctx: &Ctx) -> anyhow::Result<u64> {
    let now = ctx.clock.now();
    let mut n = 0;
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    for d in repo::needing_reminders(&mut tx, now - Duration::hours(24), now + Duration::days(2)).await? {
        let Some(user) = crux_repo::users::by_id(&mut tx, d.user_id).await? else { continue };
        let tz: Tz = user.time_zone.parse().unwrap_or(chrono_tz::Europe::Amsterdam);
        let (eve, check_in) = reminder_times(d.starts_at, tz);
        let dref = d.public_id.clone().unwrap_or_default();
        let base = reminder_payload(&d);
        if d.eve_requested_at.is_none() && now >= eve && now < d.starts_at {
            notify(&mut tx, ctx, &user.public_id, "exam.eve", &format!("exam.eve:{dref}"), eve, base.clone()).await?;
            repo::mark_eve(&mut tx, d.id, now).await?;
            n += 1;
        }
        if d.check_in_requested_at.is_none() && now >= check_in && now < check_in + Duration::hours(24) {
            notify(&mut tx, ctx, &user.public_id, "exam.checkin", &format!("exam.checkin:{dref}"), check_in, base).await?;
            repo::mark_check_in(&mut tx, d.id, now).await?;
            n += 1;
        }
    }
    tx.commit().await?;
    n += crate::plan::behind_check(ctx).await?;
    n += crate::digests::tick(ctx).await.map_err(|e| anyhow::anyhow!("digests: {e}"))?;
    Ok(n)
}

async fn notify(
    conn: &mut MySqlConnection,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reminder_times_are_local() {
        let exam = Utc.with_ymd_and_hms(2026, 11, 17, 8, 0, 0).unwrap(); // 09:00 Amsterdam
        let (eve, check_in) = reminder_times(exam, chrono_tz::Europe::Amsterdam);
        assert_eq!(eve, Utc.with_ymd_and_hms(2026, 11, 16, 17, 0, 0).unwrap());
        assert_eq!(check_in, Utc.with_ymd_and_hms(2026, 11, 17, 16, 0, 0).unwrap());
    }
}
