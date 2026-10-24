//! Check questions after a material (lecture notes: "step 4 ends with this MC") and "quiz me" on
//! a lecture or a concept. Both hand out questions without their keys; answers go to POST
//! /attempts (context Notes with the material, or Practice), which returns the key and feeds the
//! stats, weak spots and warm-ups like any other attempt.

use std::collections::HashMap;

use crux_domain::ValidationErrors;
use crux_repo::{bank, catalog, MySqlConnection};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::catalog::material_for;
use crate::courses::readable;
use crate::next::{practice_question, PracticeQuestion};
use crate::{Actor, AppResult, Ctx};

/// At most this many checks on one material, and questions in one quiz.
pub const MAX_CHECKS: usize = 20;
pub const MAX_QUIZ: i64 = 10;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaterialCheck {
    /// After this page (PDF) or section (notes); 0 = at the end.
    pub position: i32,
    pub question: PracticeQuestion,
    /// Your last outcome on this question (Right, Partial, Wrong, Blank), if you've answered it.
    pub last_outcome: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckInput {
    pub question_id: String,
    /// After this page or section; 0 (default) = at the end.
    #[serde(default)]
    pub position: i32,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChecksInput {
    /// The material's full list; replaces what was there.
    pub checks: Vec<CheckInput>,
}

/// Can `actor` answer this question? (The same rule as POST /attempts: the course bank's approved
/// questions, plus your own private ones.)
fn answerable(q: &bank::QuestionRow, user_id: i64) -> bool {
    q.archived_at.is_none()
        && (q.owner_user_id.is_none() || q.owner_user_id == Some(user_id))
        && (q.review_state == "Approved" || q.owner_user_id == Some(user_id))
}

async fn views(conn: &mut MySqlConnection, user_id: i64, rows: &[(i64, i32)]) -> AppResult<Vec<MaterialCheck>> {
    let ids: Vec<i64> = rows.iter().map(|r| r.0).collect();
    let last: HashMap<i64, Option<String>> = bank::last_outcomes(conn, user_id, &ids).await?.into_iter().collect();
    let mut out = Vec::with_capacity(rows.len());
    for (qid, position) in rows {
        let Some(q) = bank::question_by_id(conn, *qid).await? else { continue };
        if !answerable(&q, user_id) {
            continue;
        }
        if let Some(question) = practice_question(conn, *qid).await? {
            out.push(MaterialCheck { position: *position, question, last_outcome: last.get(qid).cloned().flatten() });
        }
    }
    Ok(out)
}

/// `GET /materials/{id}/checks`: the material's check questions you can answer, in reading order.
pub async fn list(ctx: &Ctx, actor: &Actor, material_ref: &str) -> AppResult<Vec<MaterialCheck>> {
    let mut conn = ctx.pool.acquire().await?;
    let (m, _) = material_for(&mut conn, actor, material_ref, false).await?;
    let rows = bank::material_checks(&mut conn, m.id).await?;
    views(&mut conn, actor.user_id, &rows).await
}

/// `PUT /materials/{id}/checks` (Owner or Editor): the course bank's questions, each after a page
/// or section. Private questions can't be attached: everyone on the course reads the material.
pub async fn set(ctx: &Ctx, actor: &Actor, material_ref: &str, input: ChecksInput) -> AppResult<Vec<MaterialCheck>> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let (m, _) = material_for(&mut tx, actor, material_ref, true).await?;
    let mut e = ValidationErrors::new();
    if input.checks.len() > MAX_CHECKS {
        e.push("checks", "too_many", "Up to 20 check questions per material.");
    }
    let mut rows: Vec<(i64, i32)> = Vec::with_capacity(input.checks.len());
    for (i, c) in input.checks.iter().enumerate() {
        if !(0..=10_000).contains(&c.position) {
            e.push(&format!("checks[{i}].position"), "range", "A page or section number, or 0 for the end.");
        }
        match bank::question(&mut tx, &c.question_id).await? {
            Some(q) if q.course_id == m.course_id && q.owner_user_id.is_none() && q.archived_at.is_none() => {
                if rows.iter().any(|r| r.0 == q.id) {
                    e.push(&format!("checks[{i}].questionId"), "duplicate", "This question is already on the list.");
                }
                rows.push((q.id, c.position));
            }
            _ => e.push(&format!("checks[{i}].questionId"), "unknown", "Pick a question from this course's bank."),
        }
    }
    e.into_result()?;
    bank::set_material_checks(&mut tx, m.id, &rows).await?;
    let saved = bank::material_checks(&mut tx, m.id).await?;
    let out = views(&mut tx, actor.user_id, &saved).await?;
    tx.commit().await?;
    Ok(out)
}

#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct QuizQuery {
    /// One lecture.
    pub component_id: Option<String>,
    /// One concept.
    pub concept_id: Option<String>,
    /// 1-10, default 2.
    pub n: Option<i64>,
    /// Comma-separated question ids already shown ("one more" skips them).
    pub exclude: Option<String>,
}

/// `GET /courses/{id}/quiz`: a couple of questions on a lecture or concept, never tried first, then
/// the ones you missed, then the longest unseen.
pub async fn quiz(ctx: &Ctx, actor: &Actor, course_ref: &str, q: QuizQuery) -> AppResult<Vec<PracticeQuestion>> {
    let mut conn = ctx.pool.acquire().await?;
    let course = readable(&mut conn, actor, course_ref).await?;
    let component_id = match &q.component_id {
        Some(r) => Some(
            catalog::component(&mut conn, r)
                .await?
                .filter(|c| c.course_id == course.id)
                .ok_or_else(|| ValidationErrors::single("componentId", "unknown", "Not a lecture of this course."))?
                .id,
        ),
        None => None,
    };
    let mut exclude = Vec::new();
    for r in q.exclude.as_deref().unwrap_or("").split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(x) = bank::question(&mut conn, r).await?.filter(|x| x.course_id == course.id) {
            exclude.push(x.id);
        }
    }
    let n = q.n.unwrap_or(2).clamp(1, MAX_QUIZ);
    let ids = bank::quiz_questions(&mut conn, actor.user_id, course.id, component_id, q.concept_id.as_deref(), &exclude, n).await?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(p) = practice_question(&mut conn, id).await? {
            out.push(p);
        }
    }
    Ok(out)
}
