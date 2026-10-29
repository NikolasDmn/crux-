//! `POST /attempts` (spec 5.9, 5.11): practice answers outside a mock (notes check questions,
//! warm-ups, practice, review). Idempotent on `clientId`, so a retried or offline-synced answer
//! counts once (review H7).

use chrono::{DateTime, Duration, Utc};
use crux_domain::grading::{self, Confidence, NumericRule, Outcome};
use crux_domain::policy::{can_edit_course, can_read_course};
use crux_domain::ValidationErrors;
use crux_repo::{bank, mocks};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::bank::{version_views, VersionView};
use crate::courses::{access_of, readable};
use crate::time::iso;
use crate::{practice, Actor, AppError, AppResult, Ctx};

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AttemptInput {
    /// Made on the device (ULID); the same id twice is the same attempt.
    pub client_id: String,
    pub question_id: String,
    /// Notes, Warmup, Practice, Review.
    pub context: String,
    /// The notes page it was asked on.
    pub material_id: Option<String>,
    #[serde(default = "one")]
    pub try_no: i8,
    pub choice_option_id: Option<String>,
    /// "Choose all that apply": every chosen option (instead of choiceOptionId).
    #[serde(default)]
    pub choice_option_ids: Option<Vec<String>>,
    pub text: Option<String>,
    pub numeric: Option<f64>,
    pub unit: Option<String>,
    pub confidence: Option<String>,
    /// Open questions: how you judged your own answer (Right, Partial, Wrong).
    pub self_outcome: Option<String>,
    /// Warm-ups are cold (spec 5.9); otherwise computed.
    pub cold: Option<bool>,
    pub answered_at: Option<DateTime<Utc>>,
}

fn one() -> i8 {
    1
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AttemptResult {
    pub client_id: String,
    pub outcome: String,
    /// Practice shows the key right away.
    pub key: VersionView,
    pub answered_at: String,
    /// true when this clientId had already been recorded (nothing changed).
    pub duplicate: bool,
}

pub async fn record(ctx: &Ctx, actor: &Actor, input: AttemptInput) -> AppResult<AttemptResult> {
    let mut e = ValidationErrors::new();
    if input.client_id.is_empty() || input.client_id.len() > 26 {
        e.push("clientId", "invalid", "Use a ULID made on the device.");
    }
    if !matches!(input.context.as_str(), "Notes" | "Warmup" | "Practice" | "Review") {
        e.push("context", "invalid", "Use Notes, Warmup, Practice or Review.");
    }
    if input.confidence.as_deref().is_some_and(|c| Confidence::from_db(c).is_none()) {
        e.push("confidence", "invalid", "Use Sure, Unsure or Guess.");
    }
    if input.try_no < 1 {
        e.push("tryNo", "invalid", "Starts at 1.");
    }
    e.into_result()?;

    let mut tx = crux_repo::begin(&ctx.pool).await?;
    let q = bank::question(&mut tx, &input.question_id).await?.ok_or(AppError::NotFound)?;
    let course_ref = crux_repo::courses::public_id_of(&mut tx, q.course_id).await?;
    let course = readable(&mut tx, actor, &course_ref).await?;
    let access = access_of(&course);
    let mine = q.owner_user_id == Some(actor.user_id);
    let visible = can_read_course(&access)
        && (q.owner_user_id.is_none() || mine)
        && (q.review_state == "Approved" || mine || (q.owner_user_id.is_none() && can_edit_course(&access)));
    if !visible {
        return Err(AppError::NotFound);
    }
    let vid = q.current_version_id.ok_or(AppError::NotFound)?;
    let versions = bank::versions_by_ids(&mut tx, &[vid]).await?;
    let key = version_views(&mut tx, &versions, &std::collections::HashMap::from([(q.id, q.qtype.clone())]))
        .await?
        .remove(&vid)
        .ok_or(AppError::NotFound)?;

    if let Some(existing) = mocks::attempt_by_client(&mut tx, actor.user_id, &input.client_id).await? {
        tx.commit().await?;
        return Ok(AttemptResult {
            client_id: existing.client_id,
            outcome: existing.outcome,
            key,
            answered_at: iso(existing.answered_at),
            duplicate: true,
        });
    }

    let now = ctx.clock.now();
    // Spec 5.11 style clamp: no future answers, and nothing older than a week.
    let at = input.answered_at.unwrap_or(now).clamp(now - Duration::days(7), now + Duration::minutes(5));
    let mut many: Vec<i64> = Vec::new();
    let (outcome, chosen) = match q.qtype.as_str() {
        "MC" => {
            let chosen = match &input.choice_option_id {
                Some(r) => Some(
                    mocks::option_id(&mut tx, vid, r)
                        .await?
                        .ok_or_else(|| ValidationErrors::single("choiceOptionId", "unknown", "Not an option of this question."))?,
                ),
                None => None,
            };
            // Several ("choose all that apply"), or the one.
            let mut picked: Vec<bool> = Vec::new();
            for r in input.choice_option_ids.as_deref().unwrap_or(&[]) {
                let (id, correct) = mocks::option_id(&mut tx, vid, r)
                    .await?
                    .ok_or_else(|| ValidationErrors::single("choiceOptionIds", "unknown", "Not an option of this question."))?;
                if !many.contains(&id) {
                    many.push(id);
                    picked.push(correct);
                }
            }
            if let Some((_, correct)) = chosen {
                if many.is_empty() {
                    picked.push(correct);
                }
            }
            let correct = key.options.as_ref().map(|os| os.iter().filter(|o| o.is_correct).count()).unwrap_or(1);
            (grading::grade_choice(&picked, correct), chosen.map(|c| c.0))
        }
        "Numeric" => {
            let n = key.numeric.as_ref().ok_or(AppError::NotFound)?;
            let rule = NumericRule {
                value: n.value,
                tolerance: n.tolerance,
                relative: n.tolerance_kind == "Relative",
                unit: n.unit.clone(),
                unit_required: n.unit_required,
            };
            (grading::grade_numeric(&rule, input.numeric, input.unit.as_deref()), None)
        }
        _ => {
            if grading::open_is_blank(input.text.as_deref()) {
                (Outcome::Blank, None)
            } else {
                match input.self_outcome.as_deref().and_then(Outcome::from_db) {
                    Some(o @ (Outcome::Right | Outcome::Partial | Outcome::Wrong)) => (o, None),
                    _ => {
                        return Err(
                            ValidationErrors::single("selfOutcome", "required", "Judge your open answer: Right, Partial or Wrong.").into()
                        )
                    }
                }
            }
        }
    };
    let material_id = match &input.material_id {
        Some(r) => crux_repo::catalog::material(&mut tx, r).await?.filter(|m| m.course_id == q.course_id).map(|m| m.id),
        None => None,
    };
    let attempt_id = mocks::insert_attempt(
        &mut tx,
        &mocks::NewAttempt {
            user_id: actor.user_id,
            client_id: &input.client_id,
            question_id: q.id,
            version_id: vid,
            material_id,
            context: &input.context,
            try_no: input.try_no,
            chosen_option_id: chosen,
            chosen_option_ids: input.choice_option_ids.is_some().then_some(many.as_slice()),
            text: input.text.as_deref(),
            numeric: input.numeric,
            confidence: input.confidence.as_deref(),
            outcome: outcome.as_db(),
            cold: input.cold.unwrap_or(false),
            at,
        },
    )
    .await?;
    practice::record(
        &mut tx,
        &practice::Answered {
            user_id: actor.user_id,
            course_id: q.course_id,
            question_id: q.id,
            component_id: q.component_id,
            outcome,
            confidence: input.confidence.as_deref().and_then(Confidence::from_db),
            at,
            source: practice::Source::Attempt { attempt_id },
            cold: if input.context == "Warmup" { Some(true) } else { input.cold },
        },
    )
    .await?;
    tx.commit().await?;
    Ok(AttemptResult { client_id: input.client_id, outcome: outcome.as_db().into(), key, answered_at: iso(at), duplicate: false })
}
