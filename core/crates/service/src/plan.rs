//! The study plan (backend-spec 5.8): rebuilds through crux_domain::plan, plan entries the user
//! edits (which pins them), and the nightly "fell behind" check.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use crux_domain::plan::{self, EntryKind, PlanCourse, PlanInput, PlanLecture};
use crux_domain::ValidationErrors;
use crux_repo::plan::{self as repo, EntryRow};
use crux_repo::{settings, study, users, MySqlConnection};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::courses::readable;
use crate::time::iso;
use crate::{Actor, AppError, AppResult, Ctx};

/// A lecture without an estimate is planned as this many hours (build choice B5-3).
pub const DEFAULT_LECTURE_HOURS: f64 = 1.5;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlanEntryView {
    pub id: String,
    pub course_id: String,
    pub course_code: String,
    pub component_id: Option<String>,
    /// Lecture, Revision, Mock, Other.
    pub kind: String,
    pub description: String,
    pub start_at: String,
    pub end_at: String,
    /// Planned, Done, Skipped, Moved.
    pub status: String,
    /// User, Planner, AI.
    pub created_by: String,
    /// Pinned entries are never moved by a rebuild (yours, or ones you edited or moved).
    pub pinned: bool,
    pub moved_to_id: Option<String>,
    pub version: i32,
}

impl From<&EntryRow> for PlanEntryView {
    fn from(e: &EntryRow) -> Self {
        Self {
            id: e.public_id.clone(),
            course_id: e.course_ref.clone(),
            course_code: e.course_code.clone(),
            component_id: e.component_ref.clone(),
            kind: e.kind.clone(),
            description: e.description.clone(),
            start_at: iso(e.start_at),
            end_at: iso(e.end_at),
            status: e.status.clone(),
            created_by: e.created_by.clone(),
            pinned: e.pinned_at.is_some() || e.created_by != "Planner",
            moved_to_id: e.moved_to_ref.clone(),
            version: e.row_version,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RebuildView {
    /// One line, e.g. "14 sessions planned through Mon 16 Nov (3 new or moved); 1 pinned kept".
    pub change_summary: String,
    /// Courses whose lectures don't all fit before their final week (also in the summary).
    pub short: Vec<ShortView>,
    /// The plan from now on.
    pub entries: Vec<PlanEntryView>,
}

/// A course whose remaining lecture hours don't fit before its final week (spec 5.8).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShortView {
    pub course_id: String,
    pub course_code: String,
    /// Lecture hours that land in the final week or don't fit at all.
    pub hours: f64,
}

async fn short_views(conn: &mut MySqlConnection, short: &[(i64, f64)], codes: &HashMap<i64, String>) -> AppResult<Vec<ShortView>> {
    let mut out = Vec::new();
    for (course_id, hours) in short {
        out.push(ShortView {
            course_id: crux_repo::courses::public_id_of(conn, *course_id).await?,
            course_code: codes.get(course_id).cloned().unwrap_or_default(),
            hours: (hours * 10.0).round() / 10.0,
        });
    }
    Ok(out)
}

struct Prefs {
    tz: Tz,
    daily_hours: f64,
    multi_course: bool,
    day_start: (u32, u32),
    day_end: (u32, u32),
}

async fn prefs(conn: &mut MySqlConnection, user_id: i64) -> AppResult<Prefs> {
    let tz = users::by_id(conn, user_id).await?.and_then(|u| u.time_zone.parse::<Tz>().ok()).unwrap_or(chrono_tz::Europe::Amsterdam);
    let extra = settings::get(conn, user_id).await?.and_then(|s| s.extra).map(|j| j.0).unwrap_or_default();
    Ok(Prefs {
        tz,
        daily_hours: extra.get("dailyHours").and_then(|v| v.as_f64()).unwrap_or(plan::DEFAULT_DAILY_HOURS),
        multi_course: extra.get("multiCoursePerDay").and_then(|v| v.as_bool()).unwrap_or(false),
        day_start: extra.get("studyStart").and_then(|v| v.as_str()).and_then(plan::parse_hhmm).unwrap_or(plan::DAY_START),
        day_end: extra.get("studyEnd").and_then(|v| v.as_str()).and_then(plan::parse_hhmm).unwrap_or(plan::DAY_END),
    })
}

/// The planner's input for one user: Active, live courses, their next exam, unfinished lectures.
async fn input(conn: &mut MySqlConnection, user_id: i64, now: DateTime<Utc>, p: &Prefs) -> AppResult<(PlanInput, HashMap<i64, String>)> {
    let exams = crux_repo::deadlines::upcoming_exams(conn, user_id, now).await?;
    let mut courses = Vec::new();
    let mut codes = HashMap::new();
    for course_id in crux_repo::flash::active_course_ids(conn, user_id).await? {
        let Some(c) = crux_repo::courses::by_internal_id(conn, user_id, course_id).await? else { continue };
        let spent: HashMap<Option<i64>, f64> = study::hours_by_component(conn, user_id, course_id).await?.into_iter().collect();
        let lectures = study::progress(conn, user_id, course_id)
            .await?
            .into_iter()
            .filter(|l| l.status.as_deref() != Some("Done"))
            .map(|l| {
                let est = l.est_hours.unwrap_or(DEFAULT_LECTURE_HOURS);
                let done = spent.get(&Some(l.component_id)).copied().unwrap_or(0.0);
                PlanLecture { component_id: l.component_id, code: l.code, name: l.name, remaining_hours: (est - done).max(0.5) }
            })
            .collect();
        let exam = exams.iter().find(|d| d.course_id == course_id);
        codes.insert(course_id, c.code.clone());
        courses.push(PlanCourse {
            course_id,
            code: c.code,
            exam: exam.map(|d| d.starts_at),
            exam_minutes: exam.and_then(|d| d.duration_min),
            lectures,
        });
    }
    let busy = repo::busy_from(conn, user_id, now).await?;
    Ok((
        PlanInput {
            now,
            tz: p.tz,
            daily_hours: p.daily_hours,
            multi_course: p.multi_course,
            day_start: p.day_start,
            day_end: p.day_end,
            courses,
            busy,
        },
        codes,
    ))
}

/// Replaces the unpinned planner entries from now on (spec 5.8), in the caller's transaction.
pub async fn rebuild_in(conn: &mut MySqlConnection, ctx: &Ctx, user_id: i64) -> AppResult<RebuildView> {
    let now = ctx.clock.now();
    let p = prefs(conn, user_id).await?;
    let existing = repo::replaceable_from(conn, user_id, now).await?;
    let before: Vec<_> = existing.iter().map(|e| (e.course_id, e.component_id, EntryKind::from_db(&e.kind), e.start_at)).collect();
    let (input, codes) = input(conn, user_id, now, &p).await?;
    let built = plan::build(&input);
    // Entries that come out identical keep their row (and id); the rest are replaced.
    let mut keep = vec![false; existing.len()];
    let mut fresh = Vec::new();
    for e in &built.entries {
        let same = existing.iter().enumerate().position(|(i, x)| {
            !keep[i]
                && x.course_id == e.course_id
                && x.component_id == e.component_id
                && x.kind == e.kind.as_db()
                && x.start_at == e.start
                && x.end_at == e.end
                && x.description == e.description
        });
        match same {
            Some(i) => keep[i] = true,
            None => fresh.push(e),
        }
    }
    for (x, kept) in existing.iter().zip(&keep) {
        if !kept {
            repo::delete(conn, x.id).await?;
        }
    }
    for e in fresh {
        repo::insert(
            conn,
            &repo::NewEntry {
                public_id: &ctx.ids.ulid(),
                user_id,
                course_id: e.course_id,
                component_id: e.component_id,
                kind: e.kind.as_db(),
                description: &e.description,
                start_at: e.start,
                end_at: e.end,
                status: "Planned",
                created_by: "Planner",
                pinned_at: None,
            },
        )
        .await?;
    }
    let summary = plan::change_summary(&before, &built, input.busy.len(), p.tz, &codes);
    let entries = repo::between(conn, user_id, now, now + Duration::days(plan::MAX_HORIZON_DAYS)).await?;
    let short = short_views(conn, &built.short, &codes).await?;
    Ok(RebuildView { change_summary: summary, short, entries: entries.iter().map(PlanEntryView::from).collect() })
}

/// `GET /plan/short`: what a rebuild would report short right now, without changing the plan
/// (Home and the plan screen show it as a warning).
pub async fn short(ctx: &Ctx, actor: &Actor) -> AppResult<Vec<ShortView>> {
    let mut conn = ctx.pool.acquire().await?;
    let p = prefs(&mut conn, actor.user_id).await?;
    let (input, codes) = input(&mut conn, actor.user_id, ctx.clock.now(), &p).await?;
    let built = plan::build(&input);
    short_views(&mut conn, &built.short, &codes).await
}

pub async fn rebuild(ctx: &Ctx, actor: &Actor) -> AppResult<RebuildView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let view = rebuild_in(&mut tx, ctx, actor.user_id).await?;
    tx.commit().await?;
    Ok(view)
}

#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PlanQuery {
    /// Default: start of today (your time zone).
    pub from: Option<DateTime<Utc>>,
    /// Default: 14 days after `from`.
    pub to: Option<DateTime<Utc>>,
    pub course_id: Option<String>,
}

pub async fn list(ctx: &Ctx, actor: &Actor, q: PlanQuery) -> AppResult<Vec<PlanEntryView>> {
    let mut conn = ctx.pool.acquire().await?;
    let p = prefs(&mut conn, actor.user_id).await?;
    let from = q.from.unwrap_or_else(|| crux_domain::fsrs::local_day_start(ctx.clock.now(), p.tz));
    let to = q.to.unwrap_or(from + Duration::days(14));
    if to <= from || to - from > Duration::days(plan::MAX_HORIZON_DAYS) {
        return Err(ValidationErrors::single("to", "range", "Pick a range of up to 120 days.").into());
    }
    let course = match &q.course_id {
        Some(r) => Some(readable(&mut conn, actor, r).await?.id),
        None => None,
    };
    Ok(repo::between(&mut conn, actor.user_id, from, to)
        .await?
        .iter()
        .filter(|e| course.is_none_or(|c| e.course_id == c))
        .map(PlanEntryView::from)
        .collect())
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntryInput {
    pub course_id: String,
    pub component_id: Option<String>,
    /// Lecture, Revision, Mock, Other (default).
    pub kind: Option<String>,
    pub description: String,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
}

fn check_entry(kind: &str, description: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> ValidationErrors {
    let mut e = ValidationErrors::new();
    if !matches!(kind, "Lecture" | "Revision" | "Mock" | "Other") {
        e.push("kind", "invalid", "Use Lecture, Revision, Mock or Other.");
    }
    if description.trim().is_empty() || description.len() > 2000 {
        e.push("description", "invalid", "Describe the entry (up to 2000 characters).");
    }
    if end <= start {
        e.push("endAt", "order", "An entry ends after it starts.");
    } else if end - start > Duration::hours(16) {
        e.push("endAt", "too_long", "Up to 16 hours.");
    }
    e
}

pub async fn create_entry(ctx: &Ctx, actor: &Actor, input: EntryInput) -> AppResult<PlanEntryView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let course = readable(&mut tx, actor, &input.course_id).await?;
    let kind = input.kind.clone().unwrap_or_else(|| "Other".into());
    check_entry(&kind, &input.description, input.start_at, input.end_at).into_result()?;
    let component_id = match &input.component_id {
        Some(r) => Some(
            crux_repo::catalog::component_ids_by_public(&mut tx, course.id, std::slice::from_ref(r))
                .await?
                .first()
                .map(|c| c.0)
                .ok_or_else(|| ValidationErrors::single("componentId", "unknown", "Pick a lecture of this course."))?,
        ),
        None => None,
    };
    let public_id = ctx.ids.ulid();
    repo::insert(
        &mut tx,
        &repo::NewEntry {
            public_id: &public_id,
            user_id: actor.user_id,
            course_id: course.id,
            component_id,
            kind: &kind,
            description: input.description.trim(),
            start_at: input.start_at,
            end_at: input.end_at,
            status: "Planned",
            created_by: "User",
            pinned_at: Some(ctx.clock.now()),
        },
    )
    .await?;
    let row = repo::by_public(&mut tx, actor.user_id, &public_id).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(PlanEntryView::from(&row))
}

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntryPatch {
    pub description: Option<String>,
    /// A new time on a Planned entry moves it: the old one becomes Moved and points at the new one,
    /// which is returned.
    pub start_at: Option<DateTime<Utc>>,
    pub end_at: Option<DateTime<Utc>>,
    /// Planned, Done, Skipped.
    pub status: Option<String>,
}

/// Any edit pins the entry (spec 5.8).
pub async fn patch_entry(ctx: &Ctx, actor: &Actor, entry_ref: &str, version: i32, p: EntryPatch) -> AppResult<PlanEntryView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let e = repo::by_public(&mut tx, actor.user_id, entry_ref).await?.ok_or(AppError::NotFound)?;
    let description = p.description.clone().unwrap_or_else(|| e.description.clone());
    let start = p.start_at.unwrap_or(e.start_at);
    let end = p.end_at.unwrap_or(if p.start_at.is_some() { start + (e.end_at - e.start_at) } else { e.end_at });
    let status = p.status.clone().unwrap_or_else(|| e.status.clone());
    let mut errs = check_entry(&e.kind, &description, start, end);
    if !matches!(status.as_str(), "Planned" | "Done" | "Skipped") && status != e.status {
        errs.push("status", "invalid", "Use Planned, Done or Skipped.");
    }
    let moving = (start != e.start_at || end != e.end_at) && e.status == "Planned";
    if (start != e.start_at || end != e.end_at) && e.status != "Planned" {
        errs.push("startAt", "not_planned", "Only planned entries can move.");
    }
    errs.into_result()?;
    if e.row_version != version {
        return Err(AppError::Conflict {
            message: "This entry changed somewhere else.".into(),
            current: Some(serde_json::to_value(PlanEntryView::from(&e)).map_err(anyhow::Error::from)?),
        });
    }
    let now = ctx.clock.now();
    let result_ref = if moving {
        let new_ref = ctx.ids.ulid();
        let new_id = repo::insert(
            &mut tx,
            &repo::NewEntry {
                public_id: &new_ref,
                user_id: actor.user_id,
                course_id: e.course_id,
                component_id: e.component_id,
                kind: &e.kind,
                description: description.trim(),
                start_at: start,
                end_at: end,
                status: &status,
                created_by: &e.created_by,
                pinned_at: Some(now),
            },
        )
        .await?;
        repo::update(&mut tx, e.id, version, &e.description, e.start_at, e.end_at, "Moved", Some(now), Some(new_id)).await?;
        crate::activity::server_event(
            &mut tx,
            actor.user_id,
            now,
            "plan.move",
            Some(e.course_id),
            e.component_id,
            Some(("StudyPlanEntry", e.id)),
            serde_json::json!({ "from": iso(e.start_at), "to": iso(start) }),
        )
        .await?;
        new_ref
    } else {
        repo::update(&mut tx, e.id, version, description.trim(), start, end, &status, Some(now), None).await?;
        e.public_id.clone()
    };
    let row = repo::by_public(&mut tx, actor.user_id, &result_ref).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(PlanEntryView::from(&row))
}

/// Your own entries are deleted; a planner entry is skipped and pinned, so a rebuild doesn't put
/// it straight back.
pub async fn delete_entry(ctx: &Ctx, actor: &Actor, entry_ref: &str) -> AppResult<()> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let e = repo::by_public(&mut tx, actor.user_id, entry_ref).await?.ok_or(AppError::NotFound)?;
    if e.created_by == "Planner" {
        repo::update(&mut tx, e.id, e.row_version, &e.description, e.start_at, e.end_at, "Skipped", Some(ctx.clock.now()), None).await?;
    } else {
        repo::delete(&mut tx, e.id).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Spec 5.8 "falls a day behind": planner entries from before the user's local today still
/// Planned become Skipped, then the plan rebuilds. Part of deadlines.tick.
pub async fn behind_check(ctx: &Ctx) -> anyhow::Result<u64> {
    let now = ctx.clock.now();
    let mut conn = ctx.pool.acquire().await?;
    // 14 h covers every time zone's midnight.
    let users = repo::users_with_planned_before(&mut conn, now - Duration::hours(14)).await?;
    drop(conn);
    let mut n = 0;
    for user_id in users {
        let mut tx = crux_repo::begin(&ctx.pool).await?;
        let p = prefs(&mut tx, user_id).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        let today = crux_domain::fsrs::local_day_start(now, p.tz);
        if repo::skip_missed(&mut tx, user_id, today).await? > 0 {
            rebuild_in(&mut tx, ctx, user_id).await.map_err(|e| anyhow::anyhow!("rebuild {user_id}: {e}"))?;
            n += 1;
        }
        tx.commit().await?;
    }
    Ok(n)
}
