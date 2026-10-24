//! Lecture progress, the `WeekSummary` read model and the course summary (backend-spec 5.7, 5.11).
//! Dashboard endpoints read only from read models (StudySession, UserQuestionStat, WeekSummary).

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use crux_domain::study::{self, LectureTime};
use crux_domain::ValidationErrors;
use crux_repo::study::{self as repo, ProgressRow};
use crux_repo::{users, MySqlConnection};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::{IntoParams, ToSchema};

use crate::courses::readable;
use crate::time::{iso, iso_opt};
use crate::{Actor, AppError, AppResult, Ctx};

// ---------- progress ----------

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LectureProgress {
    pub component_id: String,
    pub code: String,
    pub name: String,
    pub est_hours: Option<f64>,
    pub actual_hours: f64,
    /// Todo, InProgress, Done.
    pub status: String,
    pub steps_done: i32,
    pub steps_total: Option<i32>,
    pub finished_at: Option<String>,
}

fn lecture_view(p: &ProgressRow, hours: &HashMap<Option<i64>, f64>) -> LectureProgress {
    LectureProgress {
        component_id: p.component_ref.clone(),
        code: p.code.clone(),
        name: p.name.clone(),
        est_hours: p.est_hours,
        actual_hours: round1(hours.get(&Some(p.component_id)).copied().unwrap_or(0.0)),
        status: p.status.clone().unwrap_or_else(|| "Todo".into()),
        steps_done: p.steps_done.unwrap_or(0),
        steps_total: p.steps_total,
        finished_at: iso_opt(p.finished_at),
    }
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

pub async fn progress(ctx: &Ctx, actor: &Actor, course_ref: &str) -> AppResult<Vec<LectureProgress>> {
    let mut conn = ctx.pool.acquire().await?;
    let course = readable(&mut conn, actor, course_ref).await?;
    let hours: HashMap<Option<i64>, f64> = repo::hours_by_component(&mut conn, actor.user_id, course.id).await?.into_iter().collect();
    Ok(repo::progress(&mut conn, actor.user_id, course.id).await?.iter().map(|p| lecture_view(p, &hours)).collect())
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProgressPatch {
    pub component_id: String,
    /// Todo, InProgress, Done.
    pub status: Option<String>,
    pub steps_done: Option<i32>,
    pub steps_total: Option<i32>,
}

/// Finishing a lecture marks its plan entries Done and rebuilds the plan (spec 5.8).
pub async fn patch_progress(ctx: &Ctx, actor: &Actor, course_ref: &str, p: ProgressPatch) -> AppResult<LectureProgress> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let course = readable(&mut tx, actor, course_ref).await?;
    let rows = repo::progress(&mut tx, actor.user_id, course.id).await?;
    let row = rows
        .iter()
        .find(|r| r.component_ref == p.component_id)
        .ok_or_else(|| ValidationErrors::single("componentId", "unknown", "Pick a lecture of this course."))?;
    let before = row.status.clone().unwrap_or_else(|| "Todo".into());
    let steps_total = p.steps_total.or(row.steps_total);
    let mut steps_done = p.steps_done.unwrap_or(row.steps_done.unwrap_or(0));
    let status = p.status.clone().unwrap_or_else(|| {
        // Steps alone move the status along.
        match (steps_done, steps_total) {
            (d, Some(t)) if t > 0 && d >= t => "Done".into(),
            (d, _) if d > 0 && before == "Todo" => "InProgress".into(),
            _ => before.clone(),
        }
    });
    let mut e = ValidationErrors::new();
    if !matches!(status.as_str(), "Todo" | "InProgress" | "Done") {
        e.push("status", "invalid", "Use Todo, InProgress or Done.");
    }
    if steps_done < 0 || steps_total.is_some_and(|t| t < 0) {
        e.push("stepsDone", "range", "Steps can't be negative.");
    }
    e.into_result()?;
    if let Some(t) = steps_total {
        if status == "Done" {
            steps_done = t;
        }
        if steps_done > t {
            return Err(ValidationErrors::single("stepsDone", "range", "More steps done than there are.").into());
        }
    }
    if status == "Todo" {
        steps_done = 0;
    }
    let now = ctx.clock.now();
    let finished_at = if status == "Done" { row.finished_at.or(Some(now)) } else { None };
    repo::upsert_progress(&mut tx, actor.user_id, row.component_id, &status, steps_done, steps_total, finished_at).await?;
    if status == "Done" && before != "Done" {
        crux_repo::plan::done_for_component(&mut tx, actor.user_id, row.component_id, now + Duration::days(1)).await?;
    }
    if (status == "Done") != (before == "Done") {
        crate::plan::rebuild_in(&mut tx, ctx, actor.user_id).await?;
    }
    let hours: HashMap<Option<i64>, f64> = repo::hours_by_component(&mut tx, actor.user_id, course.id).await?.into_iter().collect();
    let fresh = repo::progress(&mut tx, actor.user_id, course.id).await?;
    let view = lecture_view(fresh.iter().find(|r| r.component_id == row.component_id).ok_or(AppError::NotFound)?, &hours);
    tx.commit().await?;
    Ok(view)
}

// ---------- WeekSummary ----------

pub fn iso_week_of(d: NaiveDate) -> String {
    let w = d.iso_week();
    format!("{}-W{:02}", w.year(), w.week())
}

/// Monday of an ISO week string like `2026-W43`.
pub fn monday_of(iso_week: &str) -> Option<NaiveDate> {
    let (y, w) = iso_week.split_once("-W")?;
    NaiveDate::from_isoywd_opt(y.parse().ok()?, w.parse().ok()?, chrono::Weekday::Mon)
}

fn local_midnight(tz: Tz, d: NaiveDate) -> DateTime<Utc> {
    tz.from_local_datetime(&d.and_time(NaiveTime::MIN))
        .earliest()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&d.and_time(NaiveTime::MIN)))
}

async fn tz_of(conn: &mut MySqlConnection, user_id: i64) -> AppResult<Tz> {
    Ok(users::by_id(conn, user_id).await?.and_then(|u| u.time_zone.parse().ok()).unwrap_or(chrono_tz::Europe::Amsterdam))
}

/// Per local day: per course (planned h, actual h, slips), and steps done.
type Days = BTreeMap<NaiveDate, (BTreeMap<i64, (f64, f64, i64)>, i64)>;

#[derive(Default)]
struct CourseWeek {
    planned: f64,
    actual: f64,
    planned_to_date: f64,
    mc_tries: i64,
    mc_rights: i64,
    slips: i64,
    opened: i64,
    resolved: i64,
}

/// Builds and stores one week (spec 5.11): per day per course planned/actual hours, steps done,
/// first-try MC rate, slips, weak spots opened/resolved, and on-track per course with an exam.
pub async fn rebuild_week(conn: &mut MySqlConnection, ctx: &Ctx, user_id: i64, iso_week: &str) -> AppResult<serde_json::Value> {
    let monday = monday_of(iso_week).ok_or_else(|| ValidationErrors::single("isoWeek", "invalid", "Use a week like 2026-W43."))?;
    let tz = tz_of(conn, user_id).await?;
    let now = ctx.clock.now();
    let from = local_midnight(tz, monday);
    let to = local_midnight(tz, monday + Duration::days(7));
    if to > now - crate::activity::DERIVE_WINDOW {
        crate::activity::derive_user(conn, ctx, user_id).await?;
    }
    let day_of = |t: DateTime<Utc>| t.with_timezone(&tz).date_naive();
    let mut days: Days = (0..7).map(|i| (monday + Duration::days(i), Default::default())).collect();
    let mut per_course: BTreeMap<i64, CourseWeek> = BTreeMap::new();

    for s in repo::sessions_between(conn, user_id, from, to, None).await? {
        let h = (s.ended_at - s.started_at).num_minutes() as f64 / 60.0;
        if let Some(d) = days.get_mut(&day_of(s.started_at)) {
            d.0.entry(s.course_id).or_default().1 += h;
        }
        per_course.entry(s.course_id).or_default().actual += h;
    }
    for e in crux_repo::plan::between(conn, user_id, from, to).await? {
        if e.status == "Moved" {
            continue;
        }
        let h = (e.end_at - e.start_at).num_minutes() as f64 / 60.0;
        if let Some(d) = days.get_mut(&day_of(e.start_at)) {
            d.0.entry(e.course_id).or_default().0 += h;
        }
        let c = per_course.entry(e.course_id).or_default();
        c.planned += h;
        if e.end_at <= now {
            c.planned_to_date += h;
        }
    }
    for e in crux_repo::user_events::between(conn, user_id, from, to).await? {
        let p = e.payload.as_ref().map(|j| &j.0);
        match e.event_type.as_str() {
            "next.step_done" if !p.and_then(|p| p["skipped"].as_bool()).unwrap_or(false) => {
                if let Some(d) = days.get_mut(&day_of(e.occurred_at)) {
                    d.1 += 1;
                }
            }
            "mock.graded" | "attempt" => {
                let Some(p) = p else { continue };
                if p["questionType"] == "MC" && p["firstTry"] == true {
                    if let Some(c) = e.course_id {
                        let cw = per_course.entry(c).or_default();
                        cw.mc_tries += 1;
                        cw.mc_rights += i64::from(p["outcome"] == "Right" && p["confidence"] != "Guess");
                    }
                }
            }
            _ => {}
        }
    }
    for (course, at) in repo::slips_between(conn, user_id, from, to).await? {
        if let Some(d) = days.get_mut(&day_of(at)) {
            d.0.entry(course).or_default().2 += 1;
        }
        per_course.entry(course).or_default().slips += 1;
    }
    for (course, opened, resolved) in repo::weak_spot_counts(conn, user_id, from, to).await? {
        let c = per_course.entry(course).or_default();
        c.opened = opened;
        c.resolved = resolved;
    }
    let ids: Vec<i64> = per_course.keys().copied().collect();
    let labels: HashMap<i64, (String, String)> =
        repo::course_labels(conn, &ids).await?.into_iter().map(|(id, r, c)| (id, (r, c))).collect();
    let exams = crux_repo::deadlines::upcoming_exams(conn, user_id, now).await?;
    let label = |id: &i64| labels.get(id).cloned().unwrap_or_default();

    let (mut tp, mut ta, mut steps, mut slips, mut tries, mut rights) = (0.0, 0.0, 0, 0, 0, 0);
    let days_json: Vec<_> = days
        .iter()
        .map(|(date, (courses, done))| {
            steps += done;
            json!({
                "date": date.to_string(),
                "stepsDone": done,
                "courses": courses.iter().map(|(id, (p, a, s))| json!({
                    "courseId": label(id).0, "plannedHours": round1(*p), "actualHours": round1(*a), "slips": s,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let courses_json: Vec<_> = per_course
        .iter()
        .map(|(id, c)| {
            tp += c.planned;
            ta += c.actual;
            slips += c.slips;
            tries += c.mc_tries;
            rights += c.mc_rights;
            let exam = exams.iter().find(|d| d.course_id == *id);
            json!({
                "courseId": label(id).0,
                "code": label(id).1,
                "plannedHours": round1(c.planned),
                "actualHours": round1(c.actual),
                "mcFirstTry": { "tries": c.mc_tries, "rights": c.mc_rights },
                "slips": c.slips,
                "weakSpotsOpened": c.opened,
                "weakSpotsResolved": c.resolved,
                "nextExamAt": exam.map(|d| iso(d.starts_at)),
                // B5-9: on track = at least 80% of the hours planned so far this week were studied.
                "onTrack": exam.map(|_| c.planned_to_date <= 0.0 || c.actual >= 0.8 * c.planned_to_date),
            })
        })
        .collect();
    let data = json!({
        "isoWeek": iso_week,
        "from": iso(from),
        "to": iso(to),
        "days": days_json,
        "courses": courses_json,
        "totals": {
            "plannedHours": round1(tp),
            "actualHours": round1(ta),
            "stepsDone": steps,
            "slips": slips,
            "mcFirstTryRate": if tries > 0 { Some(rights as f64 / tries as f64) } else { None },
        },
    });
    repo::upsert_week(conn, user_id, iso_week, &data, now).await?;
    Ok(data)
}

#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct WeekQuery {
    /// e.g. 2026-W43; default the current week in your time zone.
    pub iso_week: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WeekView {
    pub iso_week: String,
    /// `{isoWeek, from, to, days:[{date, stepsDone, courses:[{courseId, plannedHours, actualHours, slips}]}],
    /// courses:[{courseId, code, plannedHours, actualHours, mcFirstTry:{tries, rights}, slips, weakSpotsOpened,
    /// weakSpotsResolved, nextExamAt, onTrack}], totals:{plannedHours, actualHours, stepsDone, slips, mcFirstTryRate}}`
    #[schema(value_type = Object)]
    pub summary: serde_json::Value,
    pub rebuilt_at: String,
}

/// The stored week; the current and last week (and missing ones) are rebuilt on demand.
pub async fn week(ctx: &Ctx, actor: &Actor, q: WeekQuery) -> AppResult<WeekView> {
    let now = ctx.clock.now();
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let tz = tz_of(&mut tx, actor.user_id).await?;
    let today = now.with_timezone(&tz).date_naive();
    let w = q.iso_week.unwrap_or_else(|| iso_week_of(today));
    let monday = monday_of(&w).ok_or_else(|| ValidationErrors::single("isoWeek", "invalid", "Use a week like 2026-W43."))?;
    let recent = monday + Duration::days(14) > today;
    let stored = repo::week(&mut tx, actor.user_id, &w).await?;
    let (summary, at) = match stored {
        Some((data, at)) if !recent => (data.0, at),
        _ => (rebuild_week(&mut tx, ctx, actor.user_id, &w).await?, now),
    };
    tx.commit().await?;
    Ok(WeekView { iso_week: w, summary, rebuilt_at: iso(at) })
}

/// Nightly readmodels.rebuild (spec 10): this and last week for every user active in the last 8 days.
pub async fn rebuild_all(ctx: &Ctx) -> anyhow::Result<u64> {
    let now = ctx.clock.now();
    let mut conn = ctx.pool.acquire().await?;
    let users = crux_repo::user_events::active_users_since(&mut conn, now - Duration::days(8)).await?;
    drop(conn);
    let mut n = 0;
    for u in users {
        let mut tx = crux_repo::begin(&ctx.pool).await?;
        let tz = tz_of(&mut tx, u).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        let today = now.with_timezone(&tz).date_naive();
        for d in [today - Duration::days(7), today] {
            rebuild_week(&mut tx, ctx, u, &iso_week_of(d)).await.map_err(|e| anyhow::anyhow!("week for {u}: {e}"))?;
            n += 1;
        }
        tx.commit().await?;
    }
    Ok(n)
}

// ---------- course summary ----------

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemainingView {
    pub low: f64,
    pub mid: f64,
    pub high: f64,
    /// Actual ÷ estimate on finished lectures, once 3 are done.
    pub ratio: Option<f64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WeekRate {
    pub iso_week: String,
    pub tries: i64,
    pub rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SpotBrief {
    pub id: String,
    pub topic: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExamBrief {
    pub deadline_id: String,
    pub starts_at: String,
    pub days_left: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CourseSummary {
    pub lectures_done: usize,
    pub lectures_total: usize,
    pub hours_actual: f64,
    pub hours_estimate: f64,
    pub remaining_hours: RemainingView,
    pub next_exam: Option<ExamBrief>,
    /// First-try MC rate per week, the last 6 weeks (from WeekSummary).
    pub mc_trend: Vec<WeekRate>,
    pub mc_first_try_rate: Option<f64>,
    pub top_weak_spots: Vec<SpotBrief>,
    pub open_weak_spots: usize,
    pub lectures: Vec<LectureProgress>,
}

pub async fn summary(ctx: &Ctx, actor: &Actor, course_ref: &str) -> AppResult<CourseSummary> {
    let now = ctx.clock.now();
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let course = readable(&mut tx, actor, course_ref).await?;
    let tz = tz_of(&mut tx, actor.user_id).await?;
    let today = now.with_timezone(&tz).date_naive();
    // The current week's row is refreshed so the trend's last point is live.
    rebuild_week(&mut tx, ctx, actor.user_id, &iso_week_of(today)).await?;
    let hours: HashMap<Option<i64>, f64> = repo::hours_by_component(&mut tx, actor.user_id, course.id).await?.into_iter().collect();
    let rows = repo::progress(&mut tx, actor.user_id, course.id).await?;
    let lectures: Vec<LectureProgress> = rows.iter().map(|p| lecture_view(p, &hours)).collect();
    let times: Vec<LectureTime> = rows
        .iter()
        .map(|p| LectureTime {
            est_hours: p.est_hours,
            actual_hours: hours.get(&Some(p.component_id)).copied().unwrap_or(0.0),
            done: p.status.as_deref() == Some("Done"),
        })
        .collect();
    let r = study::remaining(&times);
    let first_week = iso_week_of(today - Duration::days(35));
    let weeks = repo::weeks(&mut tx, actor.user_id, &first_week, &iso_week_of(today)).await?;
    let mut trend = Vec::new();
    let (mut all_tries, mut all_rights) = (0, 0);
    for (w, data) in weeks {
        let c = data.0["courses"].as_array().and_then(|cs| cs.iter().find(|c| c["courseId"] == course.public_id.as_str()).cloned());
        let tries = c.as_ref().and_then(|c| c["mcFirstTry"]["tries"].as_i64()).unwrap_or(0);
        let rights = c.as_ref().and_then(|c| c["mcFirstTry"]["rights"].as_i64()).unwrap_or(0);
        all_tries += tries;
        all_rights += rights;
        trend.push(WeekRate { iso_week: w, tries, rate: (tries > 0).then(|| rights as f64 / tries as f64) });
    }
    let exam = crux_repo::deadlines::upcoming_exams(&mut tx, actor.user_id, now).await?.into_iter().find(|d| d.course_id == course.id);
    let mut spots: Vec<_> =
        crux_repo::mistakes::weak_spots(&mut tx, actor.user_id, course.id).await?.into_iter().filter(|w| w.status != "Resolved").collect();
    let rank = |s: &str| match s {
        "Critical" => 0,
        "High" => 1,
        "Medium" => 2,
        _ => 3,
    };
    spots.sort_by_key(|w| (rank(&w.severity), std::cmp::Reverse(w.last_seen_at)));
    tx.commit().await?;
    Ok(CourseSummary {
        lectures_done: times.iter().filter(|t| t.done).count(),
        lectures_total: times.len(),
        hours_actual: round1(hours.values().sum()),
        hours_estimate: round1(rows.iter().filter_map(|p| p.est_hours).sum()),
        remaining_hours: RemainingView {
            low: round1(r.low),
            mid: round1(r.mid),
            high: round1(r.high),
            ratio: r.ratio.map(|x| (x * 100.0).round() / 100.0),
        },
        next_exam: exam.map(|d| ExamBrief {
            deadline_id: d.public_id.clone().unwrap_or_default(),
            starts_at: iso(d.starts_at),
            days_left: (d.starts_at.with_timezone(&tz).date_naive() - today).num_days(),
        }),
        mc_trend: trend,
        mc_first_try_rate: (all_tries > 0).then(|| all_rights as f64 / all_tries as f64),
        open_weak_spots: spots.len(),
        top_weak_spots: spots
            .iter()
            .take(3)
            .map(|w| SpotBrief { id: w.public_id.clone(), topic: w.topic.clone(), severity: w.severity.clone() })
            .collect(),
        lectures,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_weeks() {
        assert_eq!(iso_week_of(NaiveDate::from_ymd_opt(2026, 10, 23).unwrap()), "2026-W43");
        assert_eq!(monday_of("2026-W43"), NaiveDate::from_ymd_opt(2026, 10, 19));
        assert_eq!(iso_week_of(NaiveDate::from_ymd_opt(2027, 1, 1).unwrap()), "2026-W53");
        assert_eq!(monday_of("nope"), None);
    }
}
