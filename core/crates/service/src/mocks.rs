//! Mocks (backend-spec 5.3, 5.4): build, take, grade, review. Status changes are decided by
//! crux_domain::mock (the state module) and written by repo::mocks::set_status only.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use crux_domain::builder::{self, BuildOptions, Candidate, LayoutRow};
use crux_domain::grading::{self, Confidence, NumericRule, Outcome};
use crux_domain::mock::{self, ItemReview, MockError, MockStatus};
use crux_domain::weakspot::{self as ws};
use crux_domain::ValidationErrors;
use crux_repo::courses::CourseRow;
use crux_repo::mocks::{self as repo, ItemRow, MockRow};
use crux_repo::user_events::{self, NewUserEvent};
use crux_repo::{bank, mistakes, MySqlConnection};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;

use crate::bank::{version_views, VersionView};
use crate::courses::readable;
use crate::time::{iso, iso_opt};
use crate::{practice, Actor, AppError, AppResult, Ctx};

fn wrong_state(e: MockError) -> AppError {
    match e {
        MockError::TimeOver => AppError::MockTimeOver,
        MockError::WrongState => AppError::conflict("The mock isn't in a state that allows this."),
        MockError::NotGradable => AppError::conflict("This item doesn't need a grade."),
    }
}

// ---------- views ----------

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MockSummary {
    pub id: String,
    pub course_id: String,
    pub name: String,
    /// Draft, InProgress, Submitted, Grading, Graded, Reviewed.
    pub status: String,
    pub grading_mode: String,
    pub time_limit_min: Option<i32>,
    pub question_count: usize,
    pub score: Option<f64>,
    pub max_score: Option<f64>,
    pub created_at: String,
    pub started_at: Option<String>,
    /// When the time is up (start + limit; answers are still accepted for a 2-minute grace).
    pub ends_at: Option<String>,
    pub finished_at: Option<String>,
    pub version: i32,
}

fn summary(m: &MockRow, count: usize) -> MockSummary {
    MockSummary {
        id: m.public_id.clone(),
        course_id: m.course_ref.clone(),
        name: m.name.clone(),
        status: m.status.clone(),
        grading_mode: m.grading_mode.clone(),
        time_limit_min: m.time_limit_min,
        question_count: count,
        score: m.score,
        max_score: m.max_score,
        created_at: iso(m.created_at),
        started_at: iso_opt(m.started_at),
        ends_at: m.started_at.zip(m.time_limit_min).map(|(s, l)| iso(s + chrono::Duration::minutes(l.into()))),
        finished_at: iso_opt(m.finished_at),
        version: m.row_version,
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShownOption {
    pub id: String,
    pub text: String,
    /// Only when the key is visible for this item.
    pub is_correct: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnswerView {
    pub choice_option_id: Option<String>,
    pub text: Option<String>,
    pub numeric: Option<f64>,
    pub unit: Option<String>,
    /// Sure, Unsure, Guess.
    pub confidence: Option<String>,
    pub flagged: bool,
    pub answered_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct KeyView {
    pub correct_option_id: Option<String>,
    pub value: Option<f64>,
    pub tolerance: Option<f64>,
    pub tolerance_kind: Option<String>,
    pub unit: Option<String>,
    pub model_answer: Option<String>,
    pub explanation: Option<String>,
    pub key_reliability: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    pub order: i32,
    pub question_id: String,
    #[schema(value_type = String)]
    pub r#type: String,
    pub prompt: String,
    pub image_file_id: Option<String>,
    /// MC options in this mock's shuffled order.
    pub options: Option<Vec<ShownOption>>,
    /// Numeric: the unit to answer in, when the question has one.
    pub unit: Option<String>,
    pub answer: AnswerView,
    // After grading:
    pub outcome: Option<String>,
    pub grade: Option<f64>,
    pub max_grade: f64,
    pub graded_by: Option<String>,
    pub self_shown: bool,
    pub fixed: bool,
    /// The key and explanation, only when visible (spec 5.4: revealed, self-shown, or reveal-all).
    pub key: Option<KeyView>,
    /// Wrong or partial with no slip logged yet: offer "log a slip" (spec 5.4).
    pub slip_proposed: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MockView {
    #[serde(flatten)]
    pub mock: MockSummary,
    pub reveal_all: bool,
    pub items: Vec<ItemView>,
    /// Spec 5.3 step 7.
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Warning {
    /// pool_short
    pub code: String,
    pub component_id: Option<String>,
    pub wanted: usize,
    pub got: usize,
}

async fn item_views(conn: &mut MySqlConnection, m: &MockRow, items: &[ItemRow], with_results: bool) -> AppResult<Vec<ItemView>> {
    let version_ids: Vec<i64> = items.iter().map(|i| i.question_version_id).collect();
    let versions = bank::versions_by_ids(conn, &version_ids).await?;
    let qtypes: HashMap<i64, String> = items.iter().map(|i| (i.question_id, i.qtype.clone())).collect();
    let views: HashMap<i64, VersionView> = version_views(conn, &versions, &qtypes).await?;
    let slipped: HashSet<i32> =
        if with_results { mistakes::slipped_orders(conn, m.user_id, m.id).await?.into_iter().collect() } else { HashSet::new() };
    let mut out = Vec::with_capacity(items.len());
    for it in items {
        let v = views.get(&it.question_version_id).ok_or(AppError::NotFound)?;
        let outcome = it.outcome.as_deref().and_then(Outcome::from_db);
        let review = ItemReview { outcome, revealed: it.revealed, self_shown: it.self_shown, fixed: it.fixed_in_review };
        let graded_phase = matches!(MockStatus::from_db(&m.status), MockStatus::Graded | MockStatus::Reviewed);
        let visible = with_results && graded_phase && mock::key_visible(&review, m.reveal_all);
        let order: Vec<String> = it.option_order.as_ref().and_then(|j| serde_json::from_value(j.0.clone()).ok()).unwrap_or_default();
        let options = v.options.as_ref().map(|opts| {
            let by_id: HashMap<&str, &crate::bank::OptionView> = opts.iter().map(|o| (o.id.as_str(), o)).collect();
            let ordered: Vec<&crate::bank::OptionView> = if order.is_empty() {
                opts.iter().collect()
            } else {
                order.iter().filter_map(|id| by_id.get(id.as_str()).copied()).collect()
            };
            ordered
                .into_iter()
                .map(|o| ShownOption { id: o.id.clone(), text: o.text.clone(), is_correct: visible.then_some(o.is_correct) })
                .collect()
        });
        let chosen = match it.chosen_option_id {
            Some(oid) => repo::option_public_id(conn, oid).await?,
            None => None,
        };
        let key = visible.then(|| KeyView {
            correct_option_id: v.options.as_ref().and_then(|os| os.iter().find(|o| o.is_correct).map(|o| o.id.clone())),
            value: v.numeric.as_ref().map(|n| n.value),
            tolerance: v.numeric.as_ref().map(|n| n.tolerance),
            tolerance_kind: v.numeric.as_ref().map(|n| n.tolerance_kind.clone()),
            unit: v.numeric.as_ref().and_then(|n| n.unit.clone()),
            model_answer: v
                .numeric
                .as_ref()
                .and_then(|n| n.model_answer.clone())
                .or_else(|| v.open.as_ref().and_then(|o| o.model_answer.clone())),
            explanation: v.explanation.clone(),
            key_reliability: v.key_reliability.clone(),
        });
        out.push(ItemView {
            order: it.question_order,
            question_id: it.question_ref.clone(),
            r#type: it.qtype.clone(),
            prompt: v.prompt.clone(),
            image_file_id: v.image_file_id.clone(),
            options,
            unit: v.numeric.as_ref().and_then(|n| n.unit.clone()),
            answer: AnswerView {
                choice_option_id: chosen,
                text: it.answer_text.clone(),
                numeric: it.answer_numeric,
                unit: it.answer_unit.clone(),
                confidence: it.confidence.clone(),
                flagged: it.flagged,
                answered_at: iso_opt(it.answered_at),
            },
            outcome: if with_results { it.outcome.clone() } else { None },
            grade: if with_results { it.grade } else { None },
            max_grade: it.max_grade,
            graded_by: if with_results { it.graded_by.clone() } else { None },
            self_shown: it.self_shown,
            fixed: it.fixed_in_review,
            key,
            slip_proposed: with_results
                && graded_phase
                && outcome.is_some_and(|o| ws::deserves_slip(o, None))
                && !slipped.contains(&it.question_order),
        });
    }
    Ok(out)
}

// ---------- access ----------

/// Spec 4.2 can_open_mock: the actor's own mock, and a subscription to its course. (A Passed or
/// Archived subscription still opens old mocks: history is kept; build choice.)
async fn openable(conn: &mut MySqlConnection, actor: &Actor, mock_ref: &str, lock: bool) -> AppResult<MockRow> {
    let m = repo::mock_for_user(conn, actor.user_id, mock_ref, lock).await?.ok_or(AppError::NotFound)?;
    repo::subscription_status(conn, actor.user_id, m.course_id).await?.ok_or(AppError::NotFound)?;
    Ok(m)
}

async fn subscribed_course(conn: &mut MySqlConnection, actor: &Actor, course_ref: &str) -> AppResult<CourseRow> {
    let course = readable(conn, actor, course_ref).await?;
    repo::subscription_status(conn, actor.user_id, course.id).await?.ok_or(AppError::NotFound)?;
    Ok(course)
}

async fn event(conn: &mut MySqlConnection, ctx: &Ctx, user_id: i64, kind: &str, m: &MockRow, payload: serde_json::Value) -> AppResult<()> {
    user_events::insert(
        conn,
        &NewUserEvent {
            user_id,
            actor_user_id: None,
            client_id: None,
            occurred_at: ctx.clock.now(),
            event_type: kind,
            course_id: Some(m.course_id),
            component_id: None,
            entity_type: Some("MockExam"),
            entity_id: Some(m.id),
            payload: Some(payload),
        },
    )
    .await?;
    Ok(())
}

// ---------- build ----------

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MockInput {
    pub name: Option<String>,
    /// The exam this mock rehearses; with `realLayout`, its per-lecture layout is used.
    pub deadline_id: Option<String>,
    /// Only these lectures (default: all).
    pub component_ids: Option<Vec<String>>,
    /// Number of questions without a real layout (default 20).
    pub n: Option<u32>,
    pub time_limit_min: Option<i32>,
    /// Default: your mock setting.
    pub real_layout: Option<bool>,
    /// Default: your mock setting.
    pub go_easy: Option<bool>,
    #[serde(default)]
    pub misses_only: bool,
    /// Self (default) or AI.
    pub grading_mode: Option<String>,
    /// Also your own draft questions.
    #[serde(default)]
    pub include_drafts: bool,
}

pub async fn build(ctx: &Ctx, actor: &Actor, course_ref: &str, input: MockInput) -> AppResult<MockView> {
    let mut e = ValidationErrors::new();
    if input.n.is_some_and(|n| !(1..=200).contains(&n)) {
        e.push("n", "invalid", "Use 1-200 questions.");
    }
    if input.time_limit_min.is_some_and(|t| !(1..=1440).contains(&t)) {
        e.push("timeLimitMin", "invalid", "Use 1-1440 minutes.");
    }
    if input.name.as_deref().is_some_and(|n| n.trim().is_empty() || n.chars().count() > 255) {
        e.push("name", "invalid", "Enter 1-255 characters.");
    }
    let mode = input.grading_mode.clone().unwrap_or_else(|| "Self".into());
    if !matches!(mode.as_str(), "Self" | "AI") {
        e.push("gradingMode", "invalid", "Use Self or AI.");
    }
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let course = subscribed_course(&mut tx, actor, course_ref).await?;
    e.into_result()?;
    if mode == "AI" {
        // Spec 7.1: AI grading needs the gate (consent, a key, the course allowing AI).
        crate::ai::gate(ctx, actor, "Grading", Some(&course.public_id)).await?;
    }
    let settings = crux_repo::settings::get(&mut tx, actor.user_id).await?.ok_or(AppError::NotFound)?;
    let components = match &input.component_ids {
        Some(refs) if !refs.is_empty() => {
            let found = crux_repo::catalog::component_ids_by_public(&mut tx, course.id, refs).await?;
            if found.len() != refs.len() {
                return Err(ValidationErrors::single("componentIds", "unknown", "Pick lectures of this course.").into());
            }
            Some(found.into_iter().map(|c| c.0).collect::<Vec<_>>())
        }
        _ => None,
    };
    let deadline_id = match &input.deadline_id {
        Some(r) => Some(
            crate::deadlines::deadline_id_for(&mut tx, actor.user_id, course.id, r)
                .await?
                .ok_or_else(|| ValidationErrors::single("deadlineId", "unknown", "Pick one of your exams for this course."))?,
        ),
        None => None,
    };
    let real_layout = input.real_layout.unwrap_or(settings.mock_real_layout);
    let layout = match (deadline_id, real_layout) {
        (Some(d), true) => {
            let rows = repo::deadline_layout(&mut tx, actor.user_id, course.id, d).await?;
            (!rows.is_empty()).then(|| {
                rows.into_iter()
                    .filter(|(c, _, _)| components.as_ref().is_none_or(|cs| cs.contains(c)))
                    .map(|(c, q, o)| LayoutRow { component_id: c, question_count: q.max(0) as usize, open_count: o.max(0) as usize })
                    .collect::<Vec<_>>()
            })
        }
        _ => None,
    };
    let pool = repo::pool(&mut tx, course.id, actor.user_id, input.include_drafts, components.as_deref()).await?;
    let candidates: Vec<Candidate> = pool
        .iter()
        .map(|p| {
            let stat = ws::Stat {
                attempts: p.attempts,
                rights: p.rights,
                sure_wrongs: 0,
                last_outcome: p.last_outcome.as_deref().and_then(Outcome::from_db),
                last_confidence: p.last_confidence.as_deref().and_then(Confidence::from_db),
                last_answered_at: None,
                first_right_at: None,
            };
            Candidate {
                question_id: p.question_id,
                family: p.family,
                component_id: p.component_id,
                primary_concept: p.primary_concept,
                is_open: p.is_open,
                attempts: p.attempts,
                rights: p.rights,
                last_was_miss: stat.last_was_miss(),
            }
        })
        .collect();
    let opts = BuildOptions {
        n: input.n.map(|n| n as usize),
        layout,
        go_easy: input.go_easy.unwrap_or(settings.mock_go_easy),
        misses_only: input.misses_only,
    };
    let seed = rand::random::<i64>() & i64::MAX;
    let built = builder::build(&candidates, &opts, seed as u64);
    if built.question_ids.is_empty() {
        return Err(ValidationErrors::single("pool", "empty", "No questions match. Add questions or widen the selection.").into());
    }
    let now = ctx.clock.now();
    let name = input.name.clone().unwrap_or_else(|| format!("Mock {}", now.format("%Y-%m-%d %H:%M")));
    let public_id = ctx.ids.ulid();
    let build_options = json!({
        "n": opts.n, "realLayout": opts.layout.is_some(), "goEasy": opts.go_easy, "missesOnly": opts.misses_only,
        "componentIds": input.component_ids, "includeDrafts": input.include_drafts,
    });
    let mock_id = repo::insert_mock(
        &mut tx,
        &repo::NewMock {
            public_id: &public_id,
            user_id: actor.user_id,
            course_id: course.id,
            deadline_id,
            name: name.trim(),
            time_limit_min: input.time_limit_min,
            build_options,
            seed,
            grading_mode: &mode,
        },
    )
    .await?;
    let versions: HashMap<i64, i64> = repo::current_versions(&mut tx, &built.question_ids).await?.into_iter().collect();
    let option_rows = bank::options(&mut tx, &versions.values().copied().collect::<Vec<_>>()).await?;
    let mut options_by_version: HashMap<i64, Vec<String>> = HashMap::new();
    for o in option_rows {
        options_by_version.entry(o.question_version_id).or_default().push(o.public_id.unwrap_or_default());
    }
    for (i, qid) in built.question_ids.iter().enumerate() {
        let vid = *versions.get(qid).ok_or(AppError::NotFound)?;
        let order = match options_by_version.get(&vid) {
            Some(ids) if bank::shuffle_flag(&mut tx, vid).await? => Some(json!(builder::shuffled_positions(ids.len(), seed as u64, *qid)
                .into_iter()
                .map(|p| ids[p].clone())
                .collect::<Vec<_>>())),
            Some(ids) => Some(json!(ids)),
            None => None,
        };
        repo::insert_item(&mut tx, mock_id, i as i32 + 1, *qid, vid, order).await?;
    }
    let m = repo::mock_for_user(&mut tx, actor.user_id, &public_id, false).await?.ok_or(AppError::NotFound)?;
    let items = repo::items(&mut tx, mock_id).await?;
    let mut warnings = Vec::new();
    for w in built.warnings {
        let component_id = match w.component_id {
            Some(cid) => crux_repo::catalog::component_ref(&mut tx, cid).await?,
            None => None,
        };
        warnings.push(Warning { code: "pool_short".into(), component_id, wanted: w.wanted, got: w.got });
    }
    let view = MockView {
        mock: summary(&m, items.len()),
        reveal_all: m.reveal_all,
        items: item_views(&mut tx, &m, &items, false).await?,
        warnings,
    };
    tx.commit().await?;
    Ok(view)
}

// ---------- read ----------

pub async fn list(ctx: &Ctx, actor: &Actor, course_ref: &str) -> AppResult<Vec<MockSummary>> {
    let mut conn = ctx.pool.acquire().await?;
    let course = subscribed_course(&mut conn, actor, course_ref).await?;
    let mut out = Vec::new();
    for m in repo::mocks_for_course(&mut conn, actor.user_id, course.id).await? {
        let n = repo::items(&mut conn, m.id).await?.len();
        out.push(summary(&m, n));
    }
    Ok(out)
}

/// The taking view: questions and your answers, never keys or outcomes.
pub async fn get(ctx: &Ctx, actor: &Actor, mock_ref: &str) -> AppResult<MockView> {
    let mut conn = ctx.pool.acquire().await?;
    let m = openable(&mut conn, actor, mock_ref, false).await?;
    let items = repo::items(&mut conn, m.id).await?;
    Ok(MockView {
        mock: summary(&m, items.len()),
        reveal_all: m.reveal_all,
        items: item_views(&mut conn, &m, &items, false).await?,
        warnings: vec![],
    })
}

/// Result + review: outcomes and grades; keys only where visible (spec 5.4).
pub async fn result(ctx: &Ctx, actor: &Actor, mock_ref: &str) -> AppResult<MockView> {
    let mut conn = ctx.pool.acquire().await?;
    let m = openable(&mut conn, actor, mock_ref, false).await?;
    if matches!(MockStatus::from_db(&m.status), MockStatus::Draft | MockStatus::InProgress) {
        return Err(AppError::conflict("Submit the mock first."));
    }
    let items = repo::items(&mut conn, m.id).await?;
    Ok(MockView {
        mock: summary(&m, items.len()),
        reveal_all: m.reveal_all,
        items: item_views(&mut conn, &m, &items, true).await?,
        warnings: vec![],
    })
}

// ---------- take ----------

pub async fn start(ctx: &Ctx, actor: &Actor, mock_ref: &str) -> AppResult<MockView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let m = openable(&mut tx, actor, mock_ref, true).await?;
    let next = mock::transition(MockStatus::from_db(&m.status), MockStatus::InProgress).map_err(wrong_state)?;
    repo::set_status(&mut tx, m.id, next.as_db(), Some(ctx.clock.now()), None).await?;
    event(&mut tx, ctx, actor.user_id, "mock.start", &m, json!({})).await?;
    let m = repo::mock_for_user(&mut tx, actor.user_id, mock_ref, false).await?.ok_or(AppError::NotFound)?;
    let items = repo::items(&mut tx, m.id).await?;
    let view = MockView {
        mock: summary(&m, items.len()),
        reveal_all: m.reveal_all,
        items: item_views(&mut tx, &m, &items, false).await?,
        warnings: vec![],
    };
    tx.commit().await?;
    Ok(view)
}

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnswerInput {
    pub choice_option_id: Option<String>,
    pub text: Option<String>,
    pub numeric: Option<f64>,
    pub unit: Option<String>,
    /// Sure, Unsure, Guess.
    pub confidence: Option<String>,
    #[serde(default)]
    pub flagged: bool,
}

/// Saves the whole answer for one item (the client sends it on every change).
pub async fn answer(ctx: &Ctx, actor: &Actor, mock_ref: &str, order: i32, input: AnswerInput) -> AppResult<ItemView> {
    if input.confidence.as_deref().is_some_and(|c| Confidence::from_db(c).is_none()) {
        return Err(ValidationErrors::single("confidence", "invalid", "Use Sure, Unsure or Guess.").into());
    }
    if input.text.as_deref().is_some_and(|t| t.len() > 64 * 1024) {
        return Err(ValidationErrors::single("text", "too_long", "At most 64 KB.").into());
    }
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let m = openable(&mut tx, actor, mock_ref, true).await?;
    let now = ctx.clock.now();
    mock::may_answer(MockStatus::from_db(&m.status), m.started_at, m.time_limit_min, now).map_err(wrong_state)?;
    let items = repo::items(&mut tx, m.id).await?;
    let item = items.iter().find(|i| i.question_order == order).ok_or(AppError::NotFound)?;
    let chosen = match &input.choice_option_id {
        Some(r) => Some(
            repo::option_id(&mut tx, item.question_version_id, r)
                .await?
                .ok_or_else(|| ValidationErrors::single("choiceOptionId", "unknown", "Not an option of this question."))?
                .0,
        ),
        None => None,
    };
    repo::save_answer(
        &mut tx,
        m.id,
        order,
        &repo::Answer {
            chosen_option_id: chosen,
            text: input.text.as_deref(),
            numeric: input.numeric.filter(|n| n.is_finite()),
            unit: input.unit.as_deref(),
            confidence: input.confidence.as_deref(),
            flagged: input.flagged,
            at: now,
        },
    )
    .await?;
    event(&mut tx, ctx, actor.user_id, "mock.answer", &m, json!({ "order": order })).await?;
    let fresh = repo::items(&mut tx, m.id).await?;
    let item = fresh.iter().find(|i| i.question_order == order).ok_or(AppError::NotFound)?;
    let view = item_views(&mut tx, &m, std::slice::from_ref(item), false).await?.remove(0);
    tx.commit().await?;
    Ok(view)
}

/// Auto-grades MC, Numeric and blanks (spec 5.4); open answers wait for self (or AI) grading.
pub async fn submit(ctx: &Ctx, actor: &Actor, mock_ref: &str) -> AppResult<MockView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let m = openable(&mut tx, actor, mock_ref, true).await?;
    let status = MockStatus::from_db(&m.status);
    mock::transition(status, MockStatus::Submitted).map_err(wrong_state)?;
    let items = repo::items(&mut tx, m.id).await?;
    let mut ungraded = 0usize;
    for it in &items {
        let outcome = match it.qtype.as_str() {
            "MC" => {
                let correct = match it.chosen_option_id {
                    Some(o) => repo::option_correct(&mut tx, o).await?,
                    None => None,
                };
                Some(grading::grade_mc(correct))
            }
            "Numeric" => {
                let key = bank::numeric_keys(&mut tx, &[it.question_version_id]).await?.pop().ok_or(AppError::NotFound)?;
                let rule = NumericRule {
                    value: key.value,
                    tolerance: key.tolerance,
                    relative: key.tolerance_kind == "Relative",
                    unit: key.unit,
                    unit_required: key.unit_required,
                };
                Some(grading::grade_numeric(&rule, it.answer_numeric, it.answer_unit.as_deref()))
            }
            _ if grading::open_is_blank(it.answer_text.as_deref()) => Some(Outcome::Blank),
            _ => None,
        };
        match outcome {
            Some(o) => {
                repo::set_grade(
                    &mut tx,
                    m.id,
                    it.question_order,
                    o.as_db(),
                    grading::self_grade(o, it.max_grade),
                    "Auto",
                    None,
                    o == Outcome::Right,
                )
                .await?
            }
            None => ungraded += 1,
        }
    }
    let next = mock::status_after_submit(ungraded, m.grading_mode == "AI");
    let now = ctx.clock.now();
    repo::set_status(&mut tx, m.id, MockStatus::Submitted.as_db(), None, Some(now)).await?;
    if next != MockStatus::Submitted {
        repo::set_status(&mut tx, m.id, mock::transition(MockStatus::Submitted, next).map_err(wrong_state)?.as_db(), None, None).await?;
    }
    event(&mut tx, ctx, actor.user_id, "mock.submit", &m, json!({ "ungradedOpen": ungraded })).await?;
    if next == MockStatus::Graded {
        finalize(&mut tx, ctx, actor, m.id).await?;
    }
    let view = view_after(&mut tx, actor, mock_ref).await?;
    tx.commit().await?;
    Ok(view)
}

async fn view_after(conn: &mut MySqlConnection, actor: &Actor, mock_ref: &str) -> AppResult<MockView> {
    let m = repo::mock_for_user(conn, actor.user_id, mock_ref, false).await?.ok_or(AppError::NotFound)?;
    let items = repo::items(conn, m.id).await?;
    Ok(MockView {
        mock: summary(&m, items.len()),
        reveal_all: m.reveal_all,
        items: item_views(conn, &m, &items, true).await?,
        warnings: vec![],
    })
}

/// Everything "after grading" (spec 5.4): read model, weak-spot checks, cached score, notification.
async fn finalize(conn: &mut MySqlConnection, ctx: &Ctx, actor: &Actor, mock_id: i64) -> AppResult<()> {
    let items = repo::items(conn, mock_id).await?;
    let m = crux_repo::mocks::mock_by_id(conn, mock_id).await?.ok_or(AppError::NotFound)?;
    let finished = m.finished_at.unwrap_or_else(|| ctx.clock.now());
    for it in &items {
        let Some(outcome) = it.outcome.as_deref().and_then(Outcome::from_db) else { continue };
        practice::record(
            conn,
            &practice::Answered {
                user_id: actor.user_id,
                course_id: m.course_id,
                question_id: it.question_id,
                component_id: it.component_id,
                outcome,
                confidence: it.confidence.as_deref().and_then(Confidence::from_db),
                at: it.answered_at.unwrap_or(finished),
                source: practice::Source::Mock { mock_id, order: it.question_order },
                cold: None,
            },
        )
        .await?;
    }
    let grades: Vec<(Option<f64>, f64)> = items.iter().map(|i| (i.grade, i.max_grade)).collect();
    let (score, max) = mock::score(&grades);
    repo::set_score(conn, mock_id, score, max).await?;
    crate::events::emit(
        conn,
        ctx,
        crux_domain::topics::NOTIFY_REQUESTED,
        Some(&actor.public_id),
        json!({
            "userRef": actor.public_id,
            "kind": "mock.graded",
            "dedupeKey": format!("mock.graded:{}", m.public_id),
            "dueAt": iso(ctx.clock.now()),
            "payload": { "mockRef": m.public_id, "name": m.name, "score": score, "maxScore": max },
        }),
    )
    .await
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SelfGradeInput {
    /// Right, Partial (0.5) or Wrong.
    pub outcome: String,
}

/// Spec 5.4 self mode: grade each open answer; the mock becomes Graded when all items have a grade.
pub async fn self_grade(ctx: &Ctx, actor: &Actor, mock_ref: &str, order: i32, input: SelfGradeInput) -> AppResult<MockView> {
    let outcome = match Outcome::from_db(&input.outcome) {
        Some(o @ (Outcome::Right | Outcome::Partial | Outcome::Wrong)) => o,
        _ => return Err(ValidationErrors::single("outcome", "invalid", "Use Right, Partial or Wrong.").into()),
    };
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let m = openable(&mut tx, actor, mock_ref, true).await?;
    if MockStatus::from_db(&m.status) != MockStatus::Submitted {
        return Err(wrong_state(MockError::WrongState));
    }
    let items = repo::items(&mut tx, m.id).await?;
    let item = items.iter().find(|i| i.question_order == order).ok_or(AppError::NotFound)?;
    if item.qtype != "Open" || item.graded_by.as_deref() == Some("Auto") {
        return Err(wrong_state(MockError::NotGradable));
    }
    repo::set_grade(
        &mut tx,
        m.id,
        order,
        outcome.as_db(),
        grading::self_grade(outcome, item.max_grade),
        "Self",
        None,
        outcome == Outcome::Right,
    )
    .await?;
    let remaining = repo::items(&mut tx, m.id).await?.iter().filter(|i| i.grade.is_none()).count();
    if remaining == 0 {
        repo::set_status(
            &mut tx,
            m.id,
            mock::transition(MockStatus::Submitted, MockStatus::Graded).map_err(wrong_state)?.as_db(),
            None,
            None,
        )
        .await?;
        finalize(&mut tx, ctx, actor, m.id).await?;
    }
    let view = view_after(&mut tx, actor, mock_ref).await?;
    tx.commit().await?;
    Ok(view)
}

/// Spec 5.4: show-answer sets SelfShown; fixed sets FixedInReview. When every miss is fixed or
/// shown: RevealAll and Reviewed.
pub async fn review_flag(ctx: &Ctx, actor: &Actor, mock_ref: &str, order: i32, show: bool) -> AppResult<MockView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let m = openable(&mut tx, actor, mock_ref, true).await?;
    let status = MockStatus::from_db(&m.status);
    if !matches!(status, MockStatus::Graded | MockStatus::Reviewed) {
        return Err(wrong_state(MockError::WrongState));
    }
    if !repo::set_review_flag(&mut tx, m.id, order, show, !show).await? {
        return Err(AppError::NotFound);
    }
    if status == MockStatus::Graded {
        let items = repo::items(&mut tx, m.id).await?;
        let reviews: Vec<ItemReview> = items
            .iter()
            .map(|i| ItemReview {
                outcome: i.outcome.as_deref().and_then(Outcome::from_db),
                revealed: i.revealed,
                self_shown: i.self_shown,
                fixed: i.fixed_in_review,
            })
            .collect();
        if mock::review_complete(&reviews) {
            repo::set_reveal_all(&mut tx, m.id).await?;
            repo::set_status(
                &mut tx,
                m.id,
                mock::transition(MockStatus::Graded, MockStatus::Reviewed).map_err(wrong_state)?.as_db(),
                None,
                None,
            )
            .await?;
            event(&mut tx, ctx, actor.user_id, "mock.reviewed", &m, json!({})).await?;
        }
    }
    let view = view_after(&mut tx, actor, mock_ref).await?;
    tx.commit().await?;
    Ok(view)
}

/// The last moment answers count (for the client's countdown; spec frontend 5.3 uses server time).
pub fn ends_at(m: &MockRow) -> Option<DateTime<Utc>> {
    m.started_at.and_then(|s| mock::hard_deadline(s, m.time_limit_min))
}
