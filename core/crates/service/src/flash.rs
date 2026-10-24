//! Flashcards (backend-spec 5.5): course cards and personal cards, FSRS scheduling through
//! crux_domain::fsrs, the daily queue, custom sets and offline review batches.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use crux_domain::fsrs::{self, Card, CardState};
use crux_domain::ValidationErrors;
use crux_repo::courses::CourseRow;
use crux_repo::flash::{self as repo, CardRow};
use crux_repo::{settings, users, MySqlConnection};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::courses::{access_of, readable};
use crate::time::{iso, iso_opt};
use crate::{Actor, AppError, AppResult, Ctx};

pub const MAX_BATCH: usize = 500;
const MAX_TEXT: usize = 8000;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProgressView {
    /// New, Learning, Review, Relearning.
    pub state: String,
    pub due_at: Option<String>,
    pub last_reviewed_at: Option<String>,
    pub reps: i32,
    pub lapses: i32,
    pub stability: Option<f64>,
    pub difficulty: Option<f64>,
    pub suspended: bool,
    /// Manual or CourseArchived.
    pub suspend_reason: Option<String>,
    pub warmup_flagged: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CardView {
    pub id: String,
    pub course_id: String,
    pub component_id: String,
    /// `BSA017`: unique per course, never renumbered.
    pub code: String,
    pub front: String,
    pub back: String,
    /// Slip, Lecture, Manual, Generated.
    pub source: String,
    /// Only you see personal cards; course cards are shared with everyone on the course.
    pub personal: bool,
    pub slip_id: Option<String>,
    pub progress: ProgressView,
    pub updated_at: String,
    pub version: i32,
}

impl From<&CardRow> for CardView {
    fn from(r: &CardRow) -> Self {
        Self {
            id: r.public_id.clone(),
            course_id: r.course_ref.clone(),
            component_id: r.component_ref.clone(),
            code: r.code.clone(),
            front: r.front_text.clone(),
            back: r.back_text.clone(),
            source: r.source.clone(),
            personal: r.owner_user_id.is_some(),
            slip_id: r.slip_ref.clone(),
            progress: ProgressView {
                state: r.state.clone().unwrap_or_else(|| "New".into()),
                due_at: iso_opt(r.due_at),
                last_reviewed_at: iso_opt(r.last_reviewed_at),
                reps: r.reps.unwrap_or(0),
                lapses: r.lapses.unwrap_or(0),
                stability: r.stability,
                difficulty: r.difficulty,
                suspended: r.suspended.unwrap_or(false),
                suspend_reason: r.suspend_reason.clone(),
                warmup_flagged: r.warmup_flagged_at.is_some(),
            },
            updated_at: iso(r.updated_at),
            version: r.row_version,
        }
    }
}

/// The card the actor can see (course card on a readable course, or their own personal card), or 404.
async fn visible(conn: &mut MySqlConnection, actor: &Actor, card_ref: &str) -> AppResult<(CardRow, CourseRow)> {
    let card = repo::card(conn, actor.user_id, card_ref).await?.ok_or(AppError::NotFound)?;
    if card.owner_user_id.is_some_and(|o| o != actor.user_id) {
        return Err(AppError::NotFound);
    }
    let course = readable(conn, actor, &card.course_ref).await?;
    Ok((card, course))
}

async fn reload(conn: &mut MySqlConnection, actor: &Actor, card_ref: &str) -> AppResult<CardView> {
    Ok(CardView::from(&repo::card(conn, actor.user_id, card_ref).await?.ok_or(AppError::NotFound)?))
}

#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct CardQuery {
    pub component_id: Option<String>,
    pub code: Option<String>,
}

async fn component_id(conn: &mut MySqlConnection, course_id: i64, r: &str) -> AppResult<i64> {
    Ok(crux_repo::catalog::component_ids_by_public(conn, course_id, &[r.to_string()])
        .await?
        .first()
        .map(|c| c.0)
        .ok_or_else(|| ValidationErrors::single("componentId", "unknown", "Pick a lecture of this course."))?)
}

pub async fn list(ctx: &Ctx, actor: &Actor, course_ref: &str, q: CardQuery) -> AppResult<Vec<CardView>> {
    let mut conn = ctx.pool.acquire().await?;
    let course = readable(&mut conn, actor, course_ref).await?;
    let comp = match &q.component_id {
        Some(r) => Some(component_id(&mut conn, course.id, r).await?),
        None => None,
    };
    Ok(repo::cards(&mut conn, actor.user_id, course.id, comp, q.code.as_deref()).await?.iter().map(CardView::from).collect())
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CardInput {
    pub component_id: String,
    pub front: String,
    pub back: String,
    /// true: a course card everyone on the course studies (needs Owner or Editor). Default false:
    /// a personal card only you see.
    #[serde(default)]
    pub shared: bool,
    /// Make the card from one of your slips (always personal).
    pub slip_id: Option<String>,
    /// Lecture or Manual (default). Slip is set by `slipId`; Generated only by AI.
    pub source: Option<String>,
}

fn check_text(front: &str, back: &str) -> ValidationErrors {
    let mut e = ValidationErrors::new();
    if front.trim().is_empty() || front.len() > MAX_TEXT {
        e.push("front", "invalid", "Write the front (up to 8000 characters).");
    }
    if back.trim().is_empty() || back.len() > MAX_TEXT {
        e.push("back", "invalid", "Write the back (up to 8000 characters).");
    }
    e
}

pub async fn create(ctx: &Ctx, actor: &Actor, course_ref: &str, input: CardInput) -> AppResult<CardView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let course = readable(&mut tx, actor, course_ref).await?;
    if input.shared && !crux_domain::policy::can_edit_course(&access_of(&course)) {
        return Err(AppError::NotFound);
    }
    let mut e = check_text(&input.front, &input.back);
    if input.shared && input.slip_id.is_some() {
        e.push("shared", "slip_personal", "A card made from your slip is personal.");
    }
    let source = match (&input.slip_id, input.source.as_deref()) {
        (Some(_), _) => "Slip",
        (None, None | Some("Manual")) => "Manual",
        (None, Some("Lecture")) => "Lecture",
        (None, Some(_)) => {
            e.push("source", "invalid", "Use Lecture or Manual.");
            "Manual"
        }
    };
    e.into_result()?;
    let comp = component_id(&mut tx, course.id, &input.component_id).await?;
    let slip_id = match &input.slip_id {
        Some(r) => Some(
            crux_repo::mistakes::slip(&mut tx, actor.user_id, r).await?.filter(|s| s.course_id == course.id).ok_or(AppError::NotFound)?.id,
        ),
        None => None,
    };
    repo::lock_course(&mut tx, course.id).await?;
    let code = fsrs::next_code(&course.code, &repo::codes(&mut tx, course.id).await?);
    let public_id = ctx.ids.ulid();
    repo::insert(
        &mut tx,
        &repo::NewCard {
            public_id: &public_id,
            course_id: course.id,
            component_id: comp,
            code: &code,
            front: input.front.trim(),
            back: input.back.trim(),
            source,
            owner_user_id: (!input.shared).then_some(actor.user_id),
            slip_id,
        },
    )
    .await?;
    let view = reload(&mut tx, actor, &public_id).await?;
    tx.commit().await?;
    Ok(view)
}

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CardPatch {
    pub component_id: Option<String>,
    pub front: Option<String>,
    pub back: Option<String>,
}

/// Course cards need Owner or Editor; personal cards only their owner (others get 404 either way).
pub async fn patch(ctx: &Ctx, actor: &Actor, card_ref: &str, version: i32, p: CardPatch) -> AppResult<CardView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let (card, course) = visible(&mut tx, actor, card_ref).await?;
    if card.owner_user_id.is_none() && !crux_domain::policy::can_edit_course(&access_of(&course)) {
        return Err(AppError::NotFound);
    }
    let front = p.front.as_deref().unwrap_or(&card.front_text).trim().to_string();
    let back = p.back.as_deref().unwrap_or(&card.back_text).trim().to_string();
    check_text(&front, &back).into_result()?;
    let comp = match &p.component_id {
        Some(r) => component_id(&mut tx, course.id, r).await?,
        None => card.component_id,
    };
    if card.row_version != version {
        return Err(AppError::Conflict {
            message: "This card changed somewhere else.".into(),
            current: Some(serde_json::to_value(CardView::from(&card)).map_err(anyhow::Error::from)?),
        });
    }
    repo::update(&mut tx, card.id, version, comp, &front, &back).await?;
    let view = reload(&mut tx, actor, card_ref).await?;
    tx.commit().await?;
    Ok(view)
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct FlagInput {
    pub flagged: bool,
}

/// Flag a card for the next warm-up (spec 5.9).
pub async fn set_warmup_flag(ctx: &Ctx, actor: &Actor, card_ref: &str, input: FlagInput) -> AppResult<CardView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let (card, _) = visible(&mut tx, actor, card_ref).await?;
    repo::set_warmup_flag(&mut tx, actor.user_id, card.id, input.flagged.then(|| ctx.clock.now())).await?;
    let view = reload(&mut tx, actor, card_ref).await?;
    tx.commit().await?;
    Ok(view)
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SuspendInput {
    pub suspended: bool,
}

pub async fn set_suspended(ctx: &Ctx, actor: &Actor, card_ref: &str, input: SuspendInput) -> AppResult<CardView> {
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let (card, _) = visible(&mut tx, actor, card_ref).await?;
    repo::set_suspended(&mut tx, actor.user_id, card.id, input.suspended).await?;
    let view = reload(&mut tx, actor, card_ref).await?;
    tx.commit().await?;
    Ok(view)
}

/// Called when a subscription is archived or re-activated (spec 5.5).
pub async fn on_subscription_status(conn: &mut MySqlConnection, user_id: i64, course_id: i64, archived: bool) -> AppResult<()> {
    repo::suspend_for_archive(conn, user_id, course_id, archived).await?;
    Ok(())
}

// ---------- queue ----------

struct Prefs {
    retention: f64,
    new_per_day: i32,
    review_limit: i32,
    tz: Tz,
}

async fn prefs(conn: &mut MySqlConnection, user_id: i64) -> AppResult<Prefs> {
    let s = settings::get(conn, user_id).await?;
    let tz = users::by_id(conn, user_id).await?.and_then(|u| u.time_zone.parse::<Tz>().ok()).unwrap_or(chrono_tz::Europe::Amsterdam);
    Ok(Prefs {
        retention: s.as_ref().map(|s| s.flash_retention).unwrap_or(0.9),
        new_per_day: s.as_ref().map(|s| s.flash_new_per_day).unwrap_or(20),
        review_limit: s.as_ref().map(|s| s.flash_review_limit).unwrap_or(200),
        tz,
    })
}

#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct QueueQuery {
    /// One course; default all your active courses.
    pub course_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueueView {
    /// Due cards first (oldest due first), then today's new cards.
    pub cards: Vec<CardView>,
    pub due_count: usize,
    pub new_count: usize,
    /// New cards still allowed today after this queue (FlashNewPerDay in your time zone).
    pub new_left_today: i64,
}

pub async fn queue(ctx: &Ctx, actor: &Actor, q: QueueQuery) -> AppResult<QueueView> {
    let mut conn = ctx.pool.acquire().await?;
    let course_ids = match &q.course_id {
        Some(r) => vec![readable(&mut conn, actor, r).await?.id],
        None => repo::active_course_ids(&mut conn, actor.user_id).await?,
    };
    let p = prefs(&mut conn, actor.user_id).await?;
    let now = ctx.clock.now();
    let due = repo::due(&mut conn, actor.user_id, &course_ids, now, i64::from(p.review_limit)).await?;
    let seen = repo::new_seen_since(&mut conn, actor.user_id, fsrs::local_day_start(now, p.tz)).await?;
    let left = fsrs::new_cards_left(p.new_per_day, seen);
    let fresh = repo::new_cards(&mut conn, actor.user_id, &course_ids, left).await?;
    Ok(QueueView {
        due_count: due.len(),
        new_count: fresh.len(),
        new_left_today: left - fresh.len() as i64,
        cards: due.iter().chain(fresh.iter()).map(CardView::from).collect(),
    })
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomInput {
    pub course_id: String,
    #[serde(default)]
    pub component_ids: Vec<String>,
    /// slips (cards made from slips), forgotten (lapsed at least once), unseen, all.
    pub which: String,
    /// 1-200, default 30.
    pub n: Option<i64>,
}

/// A custom set outside the schedule (spec 5.5). Reviews in it still go through FSRS.
pub async fn custom(ctx: &Ctx, actor: &Actor, input: CustomInput) -> AppResult<Vec<CardView>> {
    let mut conn = ctx.pool.acquire().await?;
    let course = readable(&mut conn, actor, &input.course_id).await?;
    let mut e = ValidationErrors::new();
    if !matches!(input.which.as_str(), "slips" | "forgotten" | "unseen" | "all") {
        e.push("which", "invalid", "Use slips, forgotten, unseen or all.");
    }
    let n = input.n.unwrap_or(30);
    if !(1..=200).contains(&n) {
        e.push("n", "range", "Between 1 and 200 cards.");
    }
    e.into_result()?;
    let mut comps = Vec::new();
    for r in &input.component_ids {
        comps.push(component_id(&mut conn, course.id, r).await?);
    }
    Ok(repo::custom(&mut conn, actor.user_id, course.id, Some(&comps), &input.which, n).await?.iter().map(CardView::from).collect())
}

#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CodeQuery {
    /// e.g. BSA017 (printed on the card, used to find it from a paper note).
    pub code: String,
}

pub async fn by_code(ctx: &Ctx, actor: &Actor, q: CodeQuery) -> AppResult<Vec<CardView>> {
    let mut conn = ctx.pool.acquire().await?;
    Ok(repo::by_code(&mut conn, actor.user_id, q.code.trim()).await?.iter().map(CardView::from).collect())
}

// ---------- reviews ----------

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReviewInput {
    /// A ULID the device made; a retry with the same id is ignored.
    pub client_id: String,
    pub flashcard_id: String,
    /// 1 Again, 2 Hard, 3 Good, 4 Easy.
    pub rating: u8,
    /// Sure, Unsure, Guess.
    pub confidence: Option<String>,
    pub reviewed_at: DateTime<Utc>,
    pub duration_ms: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ReviewBatch {
    pub reviews: Vec<ReviewInput>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReviewResult {
    pub client_id: String,
    /// applied; stale (older than the card's last review: logged, not rescheduled); duplicate
    /// (already synced); unknown (card gone or not yours: dropped).
    pub status: String,
    pub card: Option<CardView>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SlipProposal {
    pub review_client_id: String,
    pub flashcard_id: String,
    pub code: String,
    pub course_id: String,
    pub component_id: String,
    /// lapse (forgot a card you knew) or sure_wrong (Again while Sure).
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReviewBatchView {
    pub results: Vec<ReviewResult>,
    /// Confirm one with POST /slips (flashcardReviewClientId).
    pub slip_proposals: Vec<SlipProposal>,
}

fn card_of(r: &repo::ProgressRow) -> Card {
    Card {
        state: CardState::from_db(&r.state),
        stability: r.stability,
        difficulty: r.difficulty,
        due_at: r.due_at,
        last_reviewed_at: r.last_reviewed_at,
        reps: r.reps,
        lapses: r.lapses,
    }
}

/// Applies a batch (offline sync) in `reviewedAt` order, one transaction (spec 5.5).
pub async fn review(ctx: &Ctx, actor: &Actor, batch: ReviewBatch) -> AppResult<ReviewBatchView> {
    let mut e = ValidationErrors::new();
    if batch.reviews.is_empty() || batch.reviews.len() > MAX_BATCH {
        e.push("reviews", "size", "Send 1 to 500 reviews.");
    }
    let now = ctx.clock.now();
    let mut ids = HashSet::new();
    for (i, r) in batch.reviews.iter().enumerate() {
        if !(1..=4).contains(&r.rating) {
            e.push(&format!("reviews[{i}].rating"), "invalid", "Rating is 1 (Again) to 4 (Easy).");
        }
        if r.confidence.as_deref().is_some_and(|c| !matches!(c, "Sure" | "Unsure" | "Guess")) {
            e.push(&format!("reviews[{i}].confidence"), "invalid", "Use Sure, Unsure or Guess.");
        }
        if r.client_id.len() != 26 || !ids.insert(r.client_id.clone()) {
            e.push(&format!("reviews[{i}].clientId"), "invalid", "Each review needs its own 26-character id.");
        }
        if r.reviewed_at > now + chrono::Duration::minutes(5) {
            e.push(&format!("reviews[{i}].reviewedAt"), "future", "This review is in the future; check the device clock.");
        }
        if r.duration_ms.is_some_and(|d| d < 0) {
            e.push(&format!("reviews[{i}].durationMs"), "invalid", "Duration can't be negative.");
        }
    }
    e.into_result()?;

    let mut sorted = batch.reviews;
    sorted.sort_by_key(|r| r.reviewed_at);
    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let p = prefs(&mut tx, actor.user_id).await?;
    let mut results = Vec::with_capacity(sorted.len());
    let mut proposals = Vec::new();
    for r in sorted {
        if repo::review_exists(&mut tx, actor.user_id, &r.client_id).await?.is_some() {
            results.push(ReviewResult { client_id: r.client_id, status: "duplicate".into(), card: None });
            continue;
        }
        let Ok((card, course)) = visible(&mut tx, actor, &r.flashcard_id).await else {
            results.push(ReviewResult { client_id: r.client_id, status: "unknown".into(), card: None });
            continue;
        };
        let before = repo::progress_for_update(&mut tx, actor.user_id, card.id).await?;
        let next_exam = repo::next_exam(&mut tx, actor.user_id, course.id, r.reviewed_at).await?;
        let reviewed = fsrs::review(&card_of(&before), r.rating, r.reviewed_at, p.retention, p.tz, next_exam)
            .map_err(|err| anyhow::anyhow!("fsrs: {err}"))?;
        if !reviewed.stale {
            let c = reviewed.card;
            repo::set_progress(
                &mut tx,
                actor.user_id,
                card.id,
                c.state.as_db(),
                c.stability,
                c.difficulty,
                c.due_at,
                c.last_reviewed_at,
                c.reps,
                c.lapses,
                fsrs::PARAMS_VERSION,
            )
            .await?;
        }
        let elapsed = before.last_reviewed_at.map(|l| ((r.reviewed_at - l).num_milliseconds() as f64 / 86_400_000.0).max(0.0));
        repo::insert_review(
            &mut tx,
            &repo::NewReview {
                user_id: actor.user_id,
                client_id: &r.client_id,
                card_id: card.id,
                reviewed_at: r.reviewed_at,
                rating: r.rating,
                confidence: r.confidence.as_deref(),
                state_before: &before.state,
                elapsed_days: elapsed,
                duration_ms: r.duration_ms,
            },
        )
        .await?;
        let sure_wrong = r.rating == 1 && r.confidence.as_deref() == Some("Sure");
        if !reviewed.stale && (reviewed.lapsed || sure_wrong) {
            proposals.push(SlipProposal {
                review_client_id: r.client_id.clone(),
                flashcard_id: card.public_id.clone(),
                code: card.code.clone(),
                course_id: card.course_ref.clone(),
                component_id: card.component_ref.clone(),
                reason: if reviewed.lapsed { "lapse" } else { "sure_wrong" }.into(),
            });
        }
        let view = reload(&mut tx, actor, &card.public_id).await?;
        results.push(ReviewResult {
            client_id: r.client_id,
            status: if reviewed.stale { "stale" } else { "applied" }.into(),
            card: Some(view),
        });
    }
    tx.commit().await?;
    Ok(ReviewBatchView { results, slip_proposals: proposals })
}
