//! Slips, weak spots, traps, insight (backend-spec 5.4 confidence view, 5.6). All owned by the user.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use crux_domain::weakspot::{self as ws, Severity};
use crux_domain::ValidationErrors;
use crux_repo::mistakes::{self as repo, SlipRow, WeakSpotRow};
use crux_repo::{mocks, MySqlConnection};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::courses::readable;
use crate::time::{iso, iso_opt};
use crate::{Actor, AppError, AppResult, Ctx};

pub const SLIP_TYPES: [&str; 7] = ["Concept", "Formula", "Unit", "Arithmetic", "Sign", "Misread", "OptionMatching"];

// ---------- slips ----------

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SlipView {
    pub id: String,
    pub course_id: String,
    pub component_id: Option<String>,
    pub weak_spot_id: Option<String>,
    #[schema(value_type = String)]
    pub r#type: String,
    pub description: String,
    pub occurred_at: String,
    pub confidence: Option<String>,
    /// User, or AI (only after you confirmed an AI-drafted slip).
    pub logged_by: String,
    pub mock_id: Option<String>,
    pub mock_question_order: Option<i32>,
    pub resolved_at: Option<String>,
}

impl From<&SlipRow> for SlipView {
    fn from(s: &SlipRow) -> Self {
        Self {
            id: s.public_id.clone().unwrap_or_default(),
            course_id: s.course_ref.clone(),
            component_id: s.component_ref.clone(),
            weak_spot_id: s.weak_spot_ref.clone(),
            r#type: s.stype.clone(),
            description: s.description.clone(),
            occurred_at: iso(s.occurred_at),
            confidence: s.confidence.clone(),
            logged_by: s.logged_by.clone(),
            mock_id: s.mock_ref.clone(),
            mock_question_order: s.mock_question_order,
            resolved_at: iso_opt(s.resolved_at),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct SlipQuery {
    pub course_id: Option<String>,
    /// open | resolved
    pub status: Option<String>,
    #[serde(rename = "type")]
    #[param(rename = "type")]
    pub stype: Option<String>,
}

pub async fn list_slips(ctx: &Ctx, actor: &Actor, q: SlipQuery) -> AppResult<Vec<SlipView>> {
    let mut conn = ctx.pool.acquire().await?;
    let course_id = match &q.course_id {
        Some(r) => Some(readable(&mut conn, actor, r).await?.id),
        None => None,
    };
    let open = match q.status.as_deref() {
        Some("open") => Some(true),
        Some("resolved") => Some(false),
        _ => None,
    };
    Ok(repo::slips(&mut conn, actor.user_id, course_id, open, q.stype.as_deref()).await?.iter().map(SlipView::from).collect())
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SlipInput {
    pub course_id: String,
    pub component_id: Option<String>,
    pub weak_spot_id: Option<String>,
    /// Concept, Formula, Unit, Arithmetic, Sign, Misread, OptionMatching.
    #[serde(rename = "type")]
    #[schema(rename = "type")]
    pub stype: String,
    pub description: String,
    pub occurred_at: Option<DateTime<Utc>>,
    pub confidence: Option<String>,
    /// Source: a mock item (confirming a proposed slip), a practice attempt or a flashcard review
    /// (confirming a proposal from POST /flash/reviews). At most one.
    pub mock_id: Option<String>,
    pub mock_question_order: Option<i32>,
    pub attempt_client_id: Option<String>,
    pub flashcard_review_client_id: Option<String>,
}

fn check_type(stype: &str, description: &str) -> ValidationErrors {
    let mut e = ValidationErrors::new();
    if !SLIP_TYPES.contains(&stype) {
        e.push("type", "invalid", "Use one of Concept, Formula, Unit, Arithmetic, Sign, Misread, OptionMatching.");
    }
    if description.trim().is_empty() || description.len() > 4000 {
        e.push("description", "invalid", "Describe the slip (up to 4000 characters).");
    }
    e
}

pub async fn create_slip(ctx: &Ctx, actor: &Actor, input: SlipInput) -> AppResult<SlipView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let course = readable(&mut tx, actor, &input.course_id).await?;
    let mut e = check_type(&input.stype, &input.description);
    let from_mock = input.mock_id.is_some();
    let from_attempt = input.attempt_client_id.is_some();
    let from_review = input.flashcard_review_client_id.is_some();
    if !ws::one_source(from_mock, from_review, from_attempt) {
        e.push("mockId", "sources", "A slip has at most one source.");
    }
    if from_mock != input.mock_question_order.is_some() {
        e.push("mockQuestionOrder", "required", "Give the mock and the question number together.");
    }
    e.into_result()?;
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
    let (mock_id, order, mut component_from_source) = match (&input.mock_id, input.mock_question_order) {
        (Some(r), Some(o)) => {
            let m = mocks::mock_for_user(&mut tx, actor.user_id, r, false)
                .await?
                .filter(|m| m.course_id == course.id)
                .ok_or(AppError::NotFound)?;
            let item = mocks::items(&mut tx, m.id).await?.into_iter().find(|i| i.question_order == o).ok_or(AppError::NotFound)?;
            (Some(m.id), Some(o), Some(item.component_id))
        }
        _ => (None, None, None),
    };
    let attempt_id = match &input.attempt_client_id {
        Some(c) => {
            let a = mocks::attempt_by_client(&mut tx, actor.user_id, c).await?.ok_or(AppError::NotFound)?;
            component_from_source = crux_repo::bank::question_by_id(&mut tx, a.question_id).await?.map(|q| q.component_id);
            Some(a.id)
        }
        None => None,
    };
    let review_id = match &input.flashcard_review_client_id {
        Some(c) => {
            let (id, _, component_id) = crux_repo::flash::review_by_client(&mut tx, actor.user_id, c)
                .await?
                .filter(|r| r.1 == course.id)
                .ok_or(AppError::NotFound)?;
            component_from_source = Some(component_id);
            Some(id)
        }
        None => None,
    };
    let weak_spot_id = match &input.weak_spot_id {
        Some(r) => {
            Some(repo::weak_spot(&mut tx, actor.user_id, r).await?.filter(|w| w.course_id == course.id).ok_or(AppError::NotFound)?.id)
        }
        None => None,
    };
    let public_id = ctx.ids.ulid();
    repo::insert_slip(
        &mut tx,
        &repo::NewSlip {
            public_id: &public_id,
            user_id: actor.user_id,
            course_id: course.id,
            component_id: component_id.or(component_from_source),
            weak_spot_id,
            stype: &input.stype,
            description: input.description.trim(),
            occurred_at: input.occurred_at.unwrap_or_else(|| ctx.clock.now()),
            confidence: input.confidence.as_deref(),
            logged_by: "User",
            mock_id,
            mock_order: order,
            attempt_id,
            review_id,
        },
    )
    .await?;
    let row = repo::slip(&mut tx, actor.user_id, &public_id).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(SlipView::from(&row))
}

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SlipPatch {
    #[serde(rename = "type")]
    #[schema(rename = "type")]
    pub stype: Option<String>,
    pub description: Option<String>,
    /// Link to a weak spot of the same course; empty string unlinks.
    pub weak_spot_id: Option<String>,
    /// true resolves, false reopens.
    pub resolved: Option<bool>,
}

pub async fn patch_slip(ctx: &Ctx, actor: &Actor, slip_ref: &str, p: SlipPatch) -> AppResult<SlipView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let cur = repo::slip(&mut tx, actor.user_id, slip_ref).await?.ok_or(AppError::NotFound)?;
    let stype = p.stype.clone().unwrap_or_else(|| cur.stype.clone());
    let description = p.description.clone().unwrap_or_else(|| cur.description.clone());
    check_type(&stype, &description).into_result()?;
    let weak_spot_id = match p.weak_spot_id.as_deref() {
        Some("") => None,
        Some(r) => {
            Some(repo::weak_spot(&mut tx, actor.user_id, r).await?.filter(|w| w.course_id == cur.course_id).ok_or(AppError::NotFound)?.id)
        }
        None => match &cur.weak_spot_ref {
            Some(r) => repo::weak_spot(&mut tx, actor.user_id, r).await?.map(|w| w.id),
            None => None,
        },
    };
    let resolved_at = match p.resolved {
        Some(true) => Some(cur.resolved_at.unwrap_or_else(|| ctx.clock.now().max(cur.occurred_at))),
        Some(false) => None,
        None => cur.resolved_at,
    };
    repo::update_slip(&mut tx, cur.id, &stype, description.trim(), weak_spot_id, resolved_at).await?;
    let row = repo::slip(&mut tx, actor.user_id, slip_ref).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(SlipView::from(&row))
}

// ---------- weak spots ----------

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WeakSpotView {
    pub id: String,
    pub concept_id: Option<String>,
    pub topic: String,
    pub detail: Option<String>,
    /// Shown as display maths.
    pub key_rule: Option<String>,
    pub severity: String,
    /// Open, Reopened, Resolved.
    pub status: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub resolved_at: Option<String>,
    pub cold_passes_needed: i8,
    /// Cold passes in a row so far.
    pub cold_pass_run: i8,
    pub version: i32,
}

impl From<&WeakSpotRow> for WeakSpotView {
    fn from(w: &WeakSpotRow) -> Self {
        Self {
            id: w.public_id.clone(),
            concept_id: w.concept_ref.clone(),
            topic: w.topic.clone(),
            detail: w.detail.clone(),
            key_rule: w.key_rule.clone(),
            severity: w.severity.clone(),
            status: w.status.clone(),
            first_seen_at: iso(w.first_seen_at),
            last_seen_at: iso(w.last_seen_at),
            resolved_at: iso_opt(w.resolved_at),
            cold_passes_needed: w.cold_passes_needed,
            cold_pass_run: w.cold_pass_run,
            version: w.row_version,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SpotProposal {
    pub concept_id: Option<String>,
    pub concept_name: String,
    pub open_slips: usize,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WeakSpotsView {
    /// The OPEN line: Critical → Low, then most recently seen (spec 5.6).
    pub open: Vec<WeakSpotView>,
    pub resolved: Vec<WeakSpotView>,
    /// 2+ open slips on one concept within 14 days and no spot for it yet (spec 5.6).
    pub proposals: Vec<SpotProposal>,
}

pub async fn list_weak_spots(ctx: &Ctx, actor: &Actor, course_ref: &str) -> AppResult<WeakSpotsView> {
    let mut conn = ctx.pool.acquire().await?;
    let course = readable(&mut conn, actor, course_ref).await?;
    let all = repo::weak_spots(&mut conn, actor.user_id, course.id).await?;
    let (mut open, resolved): (Vec<&WeakSpotRow>, Vec<&WeakSpotRow>) = all.iter().partition(|w| w.status != "Resolved");
    open.sort_by(|a, b| {
        ws::open_line_order(
            (Severity::from_db(&a.severity).unwrap_or(Severity::Low), a.last_seen_at),
            (Severity::from_db(&b.severity).unwrap_or(Severity::Low), b.last_seen_at),
        )
    });
    let slips = repo::slip_concepts(&mut conn, actor.user_id, course.id).await?;
    let names: BTreeMap<i64, (Option<String>, String)> = slips.iter().map(|(id, r, n, _)| (*id, (r.clone(), n.clone()))).collect();
    let pairs: Vec<(i64, DateTime<Utc>)> = slips.iter().map(|(id, _, _, at)| (*id, *at)).collect();
    let covered: std::collections::HashSet<i64> = all.iter().filter(|w| w.status != "Resolved").filter_map(|w| w.concept_id).collect();
    let proposals = ws::concepts_needing_a_spot(&pairs, ctx.clock.now())
        .into_iter()
        .filter(|(c, _)| !covered.contains(c))
        .map(|(c, n)| SpotProposal {
            concept_id: names.get(&c).and_then(|x| x.0.clone()),
            concept_name: names.get(&c).map(|x| x.1.clone()).unwrap_or_default(),
            open_slips: n,
        })
        .collect();
    Ok(WeakSpotsView {
        open: open.into_iter().map(WeakSpotView::from).collect(),
        resolved: resolved.into_iter().map(WeakSpotView::from).collect(),
        proposals,
    })
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WeakSpotInput {
    pub topic: String,
    pub concept_id: Option<String>,
    pub detail: Option<String>,
    pub key_rule: Option<String>,
    /// Low, Medium (default), High, Critical.
    pub severity: Option<String>,
    /// 1-10, default 2.
    pub cold_passes_needed: Option<i8>,
}

fn check_spot(topic: &str, severity: Option<&str>, cold: Option<i8>) -> ValidationErrors {
    let mut e = ValidationErrors::new();
    if topic.trim().is_empty() || topic.chars().count() > 255 {
        e.push("topic", "invalid", "Enter 1-255 characters.");
    }
    if severity.is_some_and(|s| Severity::from_db(s).is_none()) {
        e.push("severity", "invalid", "Use Low, Medium, High or Critical.");
    }
    if cold.is_some_and(|c| !(1..=10).contains(&c)) {
        e.push("coldPassesNeeded", "invalid", "Use 1-10.");
    }
    e
}

pub async fn create_weak_spot(ctx: &Ctx, actor: &Actor, course_ref: &str, input: WeakSpotInput) -> AppResult<WeakSpotView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let course = readable(&mut tx, actor, course_ref).await?;
    check_spot(&input.topic, input.severity.as_deref(), input.cold_passes_needed).into_result()?;
    let concept_id = match &input.concept_id {
        Some(r) => Some(
            crux_repo::bank::concept_ids(&mut tx, course.id, std::slice::from_ref(r))
                .await?
                .first()
                .map(|c| c.0)
                .ok_or_else(|| ValidationErrors::single("conceptId", "unknown", "Pick a concept of this course."))?,
        ),
        None => None,
    };
    let public_id = ctx.ids.ulid();
    repo::insert_weak_spot(
        &mut tx,
        &repo::NewWeakSpot {
            public_id: &public_id,
            user_id: actor.user_id,
            course_id: course.id,
            concept_id,
            topic: input.topic.trim(),
            detail: input.detail.as_deref(),
            key_rule: input.key_rule.as_deref(),
            severity: input.severity.as_deref().unwrap_or("Medium"),
            cold_passes_needed: input.cold_passes_needed.unwrap_or(2),
            at: ctx.clock.now(),
        },
    )
    .await?;
    let row = repo::weak_spot(&mut tx, actor.user_id, &public_id).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(WeakSpotView::from(&row))
}

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WeakSpotPatch {
    pub topic: Option<String>,
    pub detail: Option<String>,
    pub key_rule: Option<String>,
    /// A manual change restarts the miss count at the new level.
    pub severity: Option<String>,
    pub cold_passes_needed: Option<i8>,
}

pub async fn patch_weak_spot(ctx: &Ctx, actor: &Actor, spot_ref: &str, version: i32, p: WeakSpotPatch) -> AppResult<WeakSpotView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let cur = repo::weak_spot(&mut tx, actor.user_id, spot_ref).await?.ok_or(AppError::NotFound)?;
    let topic = p.topic.clone().unwrap_or_else(|| cur.topic.clone());
    check_spot(&topic, p.severity.as_deref(), p.cold_passes_needed).into_result()?;
    if cur.row_version != version {
        return Err(AppError::Conflict {
            message: "Changed somewhere else.".into(),
            current: serde_json::to_value(WeakSpotView::from(&cur)).ok(),
        });
    }
    repo::update_weak_spot_fields(
        &mut tx,
        cur.id,
        version,
        topic.trim(),
        if p.detail.is_some() { p.detail.as_deref() } else { cur.detail.as_deref() },
        if p.key_rule.is_some() { p.key_rule.as_deref() } else { cur.key_rule.as_deref() },
        p.cold_passes_needed.unwrap_or(cur.cold_passes_needed),
    )
    .await?;
    if let Some(s) = &p.severity {
        if *s != cur.severity {
            repo::set_weak_spot_severity(&mut tx, cur.id, s).await?;
        }
    }
    let row = repo::weak_spot(&mut tx, actor.user_id, spot_ref).await?.ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(WeakSpotView::from(&row))
}

// ---------- traps ----------

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrapView {
    pub id: String,
    pub text: String,
    pub weak_spot_id: Option<String>,
    pub created_at: String,
}

pub async fn list_traps(ctx: &Ctx, actor: &Actor, course_ref: &str) -> AppResult<Vec<TrapView>> {
    let mut conn = ctx.pool.acquire().await?;
    let course = readable(&mut conn, actor, course_ref).await?;
    Ok(repo::traps(&mut conn, actor.user_id, course.id)
        .await?
        .into_iter()
        .map(|t| TrapView {
            id: t.public_id.unwrap_or_default(),
            text: t.text,
            weak_spot_id: t.weak_spot_ref,
            created_at: iso(t.created_at),
        })
        .collect())
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrapInput {
    pub text: String,
    pub weak_spot_id: Option<String>,
}

pub async fn create_trap(ctx: &Ctx, actor: &Actor, course_ref: &str, input: TrapInput) -> AppResult<TrapView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let course = readable(&mut tx, actor, course_ref).await?;
    if input.text.trim().is_empty() || input.text.chars().count() > 500 {
        return Err(ValidationErrors::single("text", "invalid", "Enter 1-500 characters.").into());
    }
    let weak_spot_id = match &input.weak_spot_id {
        Some(r) => {
            Some(repo::weak_spot(&mut tx, actor.user_id, r).await?.filter(|w| w.course_id == course.id).ok_or(AppError::NotFound)?.id)
        }
        None => None,
    };
    let public_id = ctx.ids.ulid();
    repo::insert_trap(&mut tx, &public_id, actor.user_id, course.id, input.text.trim(), weak_spot_id).await?;
    tx.commit().await?;
    Ok(TrapView { id: public_id, text: input.text.trim().to_string(), weak_spot_id: input.weak_spot_id, created_at: iso(ctx.clock.now()) })
}

pub async fn retire_trap(ctx: &Ctx, actor: &Actor, course_ref: &str, trap_ref: &str) -> AppResult<()> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let course = readable(&mut tx, actor, course_ref).await?;
    if !repo::retire_trap(&mut tx, actor.user_id, course.id, trap_ref, ctx.clock.now()).await? {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(())
}

// ---------- insight ----------

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PatternCourse {
    pub course_id: String,
    pub code: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Pattern {
    #[schema(value_type = String)]
    pub r#type: String,
    pub open_slips: i64,
    pub courses: Vec<PatternCourse>,
}

/// Spec 5.6: open slips grouped by type across courses, most frequent first.
pub async fn patterns(ctx: &Ctx, actor: &Actor) -> AppResult<Vec<Pattern>> {
    let mut conn = ctx.pool.acquire().await?;
    let mut by_type: BTreeMap<String, Vec<PatternCourse>> = BTreeMap::new();
    for (t, course_id, code, count) in repo::patterns(&mut conn, actor.user_id).await? {
        by_type.entry(t).or_default().push(PatternCourse { course_id, code, count });
    }
    let mut out: Vec<Pattern> =
        by_type.into_iter().map(|(t, courses)| Pattern { open_slips: courses.iter().map(|c| c.count).sum(), r#type: t, courses }).collect();
    out.sort_by(|a, b| b.open_slips.cmp(&a.open_slips));
    Ok(out)
}

#[derive(Debug, Clone, Default, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfidenceBucket {
    pub answered: i64,
    pub right: i64,
    pub right_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LectureConfidence {
    pub component_id: String,
    pub code: String,
    pub sure: ConfidenceBucket,
    pub unsure: ConfidenceBucket,
    pub guess: ConfidenceBucket,
    /// Spec 5.4: unsure + guess right rate under 50%.
    pub real_gap: bool,
}

fn bucket(answered: i64, right: i64) -> ConfidenceBucket {
    ConfidenceBucket { answered, right, right_rate: (answered > 0).then(|| (right as f64 / answered as f64 * 1000.0).round() / 1000.0) }
}

pub async fn confidence(ctx: &Ctx, actor: &Actor, course_ref: &str) -> AppResult<Vec<LectureConfidence>> {
    let mut conn = ctx.pool.acquire().await?;
    let course = readable(&mut conn, actor, course_ref).await?;
    let comps = crux_repo::catalog::components(&mut conn, course.id, false).await?;
    let mut counts: BTreeMap<(i64, String), (i64, i64)> = BTreeMap::new();
    for (cid, conf, answered, right) in mocks::confidence_counts(&mut conn, actor.user_id, course.id).await? {
        counts.insert((cid, conf), (answered, right));
    }
    Ok(comps
        .iter()
        .map(|c| {
            let get = |k: &str| counts.get(&(c.id, k.to_string())).copied().unwrap_or((0, 0));
            let (sa, sr) = get("Sure");
            let (ua, ur) = get("Unsure");
            let (ga, gr) = get("Guess");
            let low_conf_answered = ua + ga;
            LectureConfidence {
                component_id: c.public_id.clone(),
                code: c.code.clone(),
                sure: bucket(sa, sr),
                unsure: bucket(ua, ur),
                guess: bucket(ga, gr),
                real_gap: low_conf_answered > 0 && ((ur + gr) as f64) < 0.5 * low_conf_answered as f64,
            }
        })
        .collect())
}

/// Helper for tests and the warm-up (milestone 5): the open spots of a course, OPEN-line ordered.
pub async fn open_spots(conn: &mut MySqlConnection, user_id: i64, course_id: i64) -> AppResult<Vec<WeakSpotRow>> {
    let mut spots: Vec<WeakSpotRow> =
        repo::weak_spots(conn, user_id, course_id).await?.into_iter().filter(|w| w.status != "Resolved").collect();
    spots.sort_by(|a, b| {
        ws::open_line_order(
            (Severity::from_db(&a.severity).unwrap_or(Severity::Low), a.last_seen_at),
            (Severity::from_db(&b.severity).unwrap_or(Severity::Low), b.last_seen_at),
        )
    });
    Ok(spots)
}
