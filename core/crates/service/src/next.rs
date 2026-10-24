//! "Take me to what I need to do" (backend-spec 5.8) and the warm-up (5.9).

use std::collections::HashMap;

use chrono::Duration;
use chrono_tz::Tz;
use crux_domain::fsrs::local_day_start;
use crux_domain::weakspot::Severity;
use crux_repo::courses::CourseRow;
use crux_repo::{bank, study, MySqlConnection};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::bank::version_views;
use crate::courses::readable;
use crate::deadlines::DeadlineView;
use crate::flash::CardView;
use crate::{Actor, AppError, AppResult, Ctx};

pub const WARMUP_MAX: usize = 3;
const MINUTES_PER_WARMUP_ITEM: i64 = 2;
const SECONDS_PER_CARD: i64 = 10;

// ---------- warm-up ----------

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OptionText {
    pub id: String,
    pub text: String,
}

/// A question to answer, without its key (the key comes back from POST /attempts).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PracticeQuestion {
    pub id: String,
    #[schema(value_type = String)]
    pub r#type: String,
    pub prompt: String,
    pub image_file_id: Option<String>,
    pub options: Option<Vec<OptionText>>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WarmupItem {
    /// card (a warm-up-flagged flashcard) or question (one per open weak spot).
    pub kind: String,
    pub card: Option<CardView>,
    /// The mini-lesson before a flagged card (idea, worked example). Written by the AI tutor, so
    /// empty until AI is on (milestone 8).
    pub lesson: Option<String>,
    pub weak_spot_id: Option<String>,
    pub topic: Option<String>,
    pub key_rule: Option<String>,
    pub question: Option<PracticeQuestion>,
}

fn severity_rank(s: &str) -> u8 {
    match Severity::from_db(s) {
        Some(Severity::Critical) => 0,
        Some(Severity::High) => 1,
        Some(Severity::Medium) => 2,
        _ => 3,
    }
}

async fn practice_question(conn: &mut MySqlConnection, question_id: i64) -> AppResult<Option<PracticeQuestion>> {
    let Some(q) = bank::question_by_id(conn, question_id).await? else { return Ok(None) };
    let Some(vid) = q.current_version_id else { return Ok(None) };
    let versions = bank::versions_by_ids(conn, &[vid]).await?;
    let types = HashMap::from([(q.id, q.qtype.clone())]);
    let mut views = version_views(conn, &versions, &types).await?;
    let Some(v) = views.remove(&vid) else { return Ok(None) };
    Ok(Some(PracticeQuestion {
        id: q.public_id,
        r#type: q.qtype,
        prompt: v.prompt,
        image_file_id: v.image_file_id,
        options: v.options.map(|os| os.into_iter().map(|o| OptionText { id: o.id, text: o.text }).collect()),
        unit: v.numeric.and_then(|n| n.unit),
    }))
}

/// Spec 5.9: flagged cards first, then one question per open weak spot (highest severity first).
pub async fn warmup_items(conn: &mut MySqlConnection, ctx: &Ctx, user_id: i64, course: &CourseRow) -> AppResult<Vec<WarmupItem>> {
    let mut items = Vec::new();
    for c in crux_repo::flash::cards(conn, user_id, course.id, None, None).await? {
        if items.len() >= WARMUP_MAX {
            break;
        }
        if c.warmup_flagged_at.is_some() && !c.suspended.unwrap_or(false) {
            items.push(WarmupItem {
                kind: "card".into(),
                card: Some(CardView::from(&c)),
                lesson: None,
                weak_spot_id: None,
                topic: None,
                key_rule: None,
                question: None,
            });
        }
    }
    let mut spots: Vec<_> =
        crux_repo::mistakes::weak_spots(conn, user_id, course.id).await?.into_iter().filter(|w| w.status != "Resolved").collect();
    spots.sort_by_key(|w| (severity_rank(&w.severity), std::cmp::Reverse(w.last_seen_at)));
    let mut used = Vec::new();
    for w in spots {
        if items.len() >= WARMUP_MAX {
            break;
        }
        let Some(concept) = w.concept_id else { continue };
        let since = ctx.clock.now() - Duration::days(7);
        let Some(qid) = study::warmup_question(conn, user_id, course.id, concept, since, &used).await? else { continue };
        used.push(qid);
        if let Some(q) = practice_question(conn, qid).await? {
            items.push(WarmupItem {
                kind: "question".into(),
                card: None,
                lesson: None,
                weak_spot_id: Some(w.public_id.clone()),
                topic: Some(w.topic.clone()),
                key_rule: w.key_rule.clone(),
                question: Some(q),
            });
        }
    }
    Ok(items)
}

async fn subscribed(conn: &mut MySqlConnection, actor: &Actor, course_ref: &str) -> AppResult<CourseRow> {
    let course = readable(conn, actor, course_ref).await?;
    match crux_repo::deadlines::subscription_id(conn, actor.user_id, course.id).await? {
        Some((_, status)) if status == "Active" => Ok(course),
        _ => Err(AppError::NotFound),
    }
}

/// `GET /courses/{id}/warmup`. Answers go to POST /attempts (context Warmup, cold) or
/// POST /flash/reviews (which clears the card's flag).
pub async fn warmup(ctx: &Ctx, actor: &Actor, course_ref: &str) -> AppResult<Vec<WarmupItem>> {
    let mut conn = ctx.pool.acquire().await?;
    let course = subscribed(&mut conn, actor, course_ref).await?;
    warmup_items(&mut conn, ctx, actor.user_id, &course).await
}

// ---------- /next ----------

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    /// course, component, planEntry
    #[schema(value_type = String)]
    pub r#type: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// warmup, lecture, mock, flashcards, review
    pub kind: String,
    pub title: String,
    pub est_minutes: i64,
    pub target: Target,
    /// The plan entry this step comes from; finishing the step marks it Done.
    pub plan_entry_id: Option<String>,
    /// The lecture, for lecture steps.
    pub component_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CourseRef {
    pub id: String,
    pub code: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NextView {
    pub course: Option<CourseRef>,
    pub steps: Vec<Step>,
    /// One line, e.g. "BSA exam in 6 days, L4 is next, 2 open weak spots".
    pub reason: String,
    /// Exams that have started with no result yet: "how did it go?" (spec 5.10).
    pub check_ins: Vec<DeadlineView>,
}

fn tz_of(u: Option<&crux_repo::users::UserRow>) -> Tz {
    u.and_then(|u| u.time_zone.parse().ok()).unwrap_or(chrono_tz::Europe::Amsterdam)
}

pub async fn next(ctx: &Ctx, actor: &Actor) -> AppResult<NextView> {
    let now = ctx.clock.now();
    let mut conn = ctx.pool.acquire().await?;
    let tz = tz_of(crux_repo::users::by_id(&mut conn, actor.user_id).await?.as_ref());
    let check_ins = crate::deadlines::awaiting_result(&mut conn, actor.user_id, now).await?;
    let active = crux_repo::flash::active_course_ids(&mut conn, actor.user_id).await?;
    let day_start = local_day_start(now, tz);
    let today: Vec<_> = crux_repo::plan::between(&mut conn, actor.user_id, day_start, day_start + Duration::days(1))
        .await?
        .into_iter()
        .filter(|e| e.status == "Planned" && active.contains(&e.course_id))
        .collect();
    let exams = crux_repo::deadlines::upcoming_exams(&mut conn, actor.user_id, now).await?;

    // The target course: today's plan, else the nearest exam with lectures left, else any active course.
    let mut main: Option<Step> = None;
    let mut course_id = None;
    let mut next_lecture_code = None;
    if let Some(e) = today.first() {
        course_id = Some(e.course_id);
        let kind = match e.kind.as_str() {
            "Mock" => "mock",
            "Revision" => "review",
            _ if e.component_id.is_some() => "lecture",
            _ => "review",
        };
        main = Some(Step {
            kind: kind.into(),
            title: e.description.clone(),
            est_minutes: (e.end_at - e.start_at).num_minutes(),
            target: Target { r#type: "planEntry".into(), id: e.public_id.clone() },
            plan_entry_id: Some(e.public_id.clone()),
            component_id: e.component_ref.clone(),
        });
    }
    let mut ordered: Vec<i64> = Vec::new();
    for c in exams.iter().map(|d| d.course_id).filter(|c| active.contains(c)).chain(active.iter().copied()) {
        if !ordered.contains(&c) {
            ordered.push(c);
        }
    }
    if main.is_none() {
        for c in &ordered {
            let lectures = study::progress(&mut conn, actor.user_id, *c).await?;
            let next_lec = lectures
                .iter()
                .find(|l| l.status.as_deref() == Some("InProgress"))
                .or_else(|| lectures.iter().find(|l| l.status.as_deref() != Some("Done")));
            if let Some(l) = next_lec {
                course_id = Some(*c);
                next_lecture_code = Some(l.code.clone());
                main = Some(Step {
                    kind: "lecture".into(),
                    title: format!("{} {}", l.code, l.name),
                    est_minutes: ((l.est_hours.unwrap_or(crate::plan::DEFAULT_LECTURE_HOURS)) * 60.0).round() as i64,
                    target: Target { r#type: "component".into(), id: l.component_ref.clone() },
                    plan_entry_id: None,
                    component_id: Some(l.component_ref.clone()),
                });
                break;
            }
        }
    }
    if course_id.is_none() {
        for c in &ordered {
            if study::due_cards(&mut conn, actor.user_id, *c, now).await? > 0 {
                course_id = Some(*c);
                break;
            }
        }
    }
    let Some(course_id) = course_id.or(ordered.first().copied()) else {
        return Ok(NextView {
            course: None,
            steps: Vec::new(),
            reason: "Nothing to do yet: add a course and its exam to get a plan.".into(),
            check_ins,
        });
    };
    let course = crux_repo::courses::by_internal_id(&mut conn, actor.user_id, course_id).await?.ok_or(AppError::NotFound)?;
    let mut steps = Vec::new();

    let warm = warmup_items(&mut conn, ctx, actor.user_id, &course).await?;
    let open_spots =
        crux_repo::mistakes::weak_spots(&mut conn, actor.user_id, course_id).await?.iter().filter(|w| w.status != "Resolved").count();
    if !warm.is_empty() {
        steps.push(Step {
            kind: "warmup".into(),
            title: format!("Warm-up: {} item{}", warm.len(), if warm.len() == 1 { "" } else { "s" }),
            est_minutes: warm.len() as i64 * MINUTES_PER_WARMUP_ITEM,
            target: Target { r#type: "course".into(), id: course.public_id.clone() },
            plan_entry_id: None,
            component_id: None,
        });
    }
    if let Some(m) = main {
        steps.push(m);
    }
    let due = study::due_cards(&mut conn, actor.user_id, course_id, now).await?;
    if due > 0 {
        steps.push(Step {
            kind: "flashcards".into(),
            title: format!("{due} due flashcard{}", if due == 1 { "" } else { "s" }),
            est_minutes: ((due * SECONDS_PER_CARD) as f64 / 60.0).ceil().max(1.0) as i64,
            target: Target { r#type: "course".into(), id: course.public_id.clone() },
            plan_entry_id: None,
            component_id: None,
        });
    }

    let mut reason = Vec::new();
    if let Some(d) = exams.iter().find(|d| d.course_id == course_id) {
        let days = (d.starts_at.with_timezone(&tz).date_naive() - now.with_timezone(&tz).date_naive()).num_days();
        let what = if d.dtype == "Resit" { "resit" } else { "exam" };
        reason.push(match days {
            0 => format!("{} {what} today", course.code),
            1 => format!("{} {what} tomorrow", course.code),
            n => format!("{} {what} in {n} days", course.code),
        });
    } else {
        reason.push(course.code.clone());
    }
    if let Some(e) = today.first().filter(|e| e.course_id == course_id) {
        reason.push(format!("today's plan: {}", e.description));
    } else if let Some(code) = next_lecture_code {
        reason.push(format!("{code} is next"));
    }
    if open_spots > 0 {
        reason.push(format!("{open_spots} open weak spot{}", if open_spots == 1 { "" } else { "s" }));
    }
    if due > 0 && steps.len() == 1 {
        reason.push(format!("{due} cards due"));
    }
    Ok(NextView {
        course: Some(CourseRef {
            id: course.public_id.clone(),
            code: course.code.clone(),
            name: course.name.clone(),
            color: course.color.clone(),
        }),
        steps,
        reason: reason.join(", "),
        check_ins,
    })
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdvanceInput {
    pub step_index: i32,
    /// The step's kind and target as /next returned them.
    pub kind: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub plan_entry_id: Option<String>,
    /// true when the step was skipped rather than done.
    #[serde(default)]
    pub skipped: bool,
}

/// Logs `next.step_done`; a finished plan-entry step marks the entry Done.
pub async fn advance(ctx: &Ctx, actor: &Actor, input: AdvanceInput) -> AppResult<()> {
    let mut e = crux_domain::ValidationErrors::new();
    if !(0..=20).contains(&input.step_index) {
        e.push("stepIndex", "range", "Step 0 to 20.");
    }
    if !matches!(input.kind.as_str(), "warmup" | "lecture" | "mock" | "flashcards" | "review") {
        e.push("kind", "invalid", "Use warmup, lecture, mock, flashcards or review.");
    }
    e.into_result()?;
    let now = ctx.clock.now();
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let mut course_id = None;
    let mut component_id = None;
    if let Some(r) = &input.plan_entry_id {
        let entry = crux_repo::plan::by_public(&mut tx, actor.user_id, r).await?.ok_or(AppError::NotFound)?;
        course_id = Some(entry.course_id);
        component_id = entry.component_id;
        if !input.skipped && entry.status == "Planned" {
            crux_repo::plan::update(
                &mut tx,
                entry.id,
                entry.row_version,
                &entry.description,
                entry.start_at,
                entry.end_at,
                "Done",
                Some(now),
                None,
            )
            .await?;
        }
    } else if let (Some("course"), Some(r)) = (input.target_type.as_deref(), &input.target_id) {
        course_id = Some(readable(&mut tx, actor, r).await?.id);
    } else if let (Some("component"), Some(r)) = (input.target_type.as_deref(), &input.target_id) {
        let k = crux_repo::catalog::component(&mut tx, r).await?.ok_or(AppError::NotFound)?;
        let cref = crux_repo::courses::public_id_of(&mut tx, k.course_id).await?;
        course_id = Some(readable(&mut tx, actor, &cref).await?.id);
        component_id = Some(k.id);
    }
    crate::activity::server_event(
        &mut tx,
        actor.user_id,
        now,
        "next.step_done",
        course_id,
        component_id,
        None,
        serde_json::json!({ "stepIndex": input.step_index, "kind": input.kind, "skipped": input.skipped }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
