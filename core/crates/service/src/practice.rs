//! What every graded answer does (spec 5.4 "after grading", 5.6, 5.11), whether it came from a mock,
//! a practice attempt or a warm-up: update UserQuestionStat in the same transaction, resolve slips
//! on a right answer, and add a WeakSpotCheck to the open spots of the question's concepts.

use chrono::{DateTime, Duration, Utc};
use crux_domain::grading::{Confidence, Outcome};
use crux_domain::weakspot::{self, Check, Progress, Severity, Stat, WsStatus};
use crux_repo::mocks::StatRow;
use crux_repo::{bank, mistakes, mocks, MySqlConnection};

use crate::AppResult;

#[derive(Debug, Clone, Copy)]
pub enum Source {
    Mock { mock_id: i64, order: i32 },
    Attempt { attempt_id: i64 },
}

pub struct Answered {
    pub user_id: i64,
    pub course_id: i64,
    pub question_id: i64,
    pub component_id: i64,
    pub outcome: Outcome,
    pub confidence: Option<Confidence>,
    pub at: DateTime<Utc>,
    pub source: Source,
    /// Warm-ups are cold by definition (spec 5.9); otherwise computed.
    pub cold: Option<bool>,
}

pub(crate) fn stat_from_row(r: Option<StatRow>) -> Stat {
    match r {
        None => Stat::EMPTY,
        Some(r) => Stat {
            attempts: r.attempts,
            rights: r.rights,
            sure_wrongs: r.sure_wrongs,
            last_outcome: r.last_outcome.as_deref().and_then(Outcome::from_db),
            last_confidence: r.last_confidence.as_deref().and_then(Confidence::from_db),
            last_answered_at: r.last_answered_at,
            first_right_at: r.first_right_at,
        },
    }
}

pub(crate) fn stat_to_row(s: &Stat) -> StatRow {
    StatRow {
        attempts: s.attempts,
        rights: s.rights,
        sure_wrongs: s.sure_wrongs,
        last_outcome: s.last_outcome.map(|o| o.as_db().to_string()),
        last_confidence: s.last_confidence.map(|c| c.as_db().to_string()),
        last_answered_at: s.last_answered_at,
        first_right_at: s.first_right_at,
    }
}

/// Spec 10 `readmodels.rebuild`: rebuilds the user's `UserQuestionStat` rows from the answer log
/// and repairs any that drifted. `Stat::apply` is order-free (sums, newest-wins, earliest right),
/// so replaying the log oldest first gives what the incremental updates should have produced.
/// Returns the number of rows repaired.
pub async fn verify_stats(conn: &mut MySqlConnection, user_id: i64) -> AppResult<u64> {
    use std::collections::BTreeMap;
    let mut want: BTreeMap<i64, Stat> = BTreeMap::new();
    for (q, outcome, confidence, at) in mocks::answer_log(conn, user_id).await? {
        let Some(outcome) = Outcome::from_db(&outcome) else { continue };
        let s = want.remove(&q).unwrap_or(Stat::EMPTY);
        want.insert(q, s.apply(outcome, confidence.as_deref().and_then(Confidence::from_db), at));
    }
    let mut have: BTreeMap<i64, Stat> =
        mocks::stats_of_user(conn, user_id).await?.into_iter().map(|(q, r)| (q, stat_from_row(Some(r)))).collect();
    let mut repaired = 0;
    for (q, s) in &want {
        if have.remove(q).as_ref() != Some(s) {
            mocks::upsert_stat(conn, user_id, *q, &stat_to_row(s)).await?;
            repaired += 1;
        }
    }
    for q in have.into_keys() {
        mocks::delete_stat(conn, user_id, q).await?;
        repaired += 1;
    }
    Ok(repaired)
}

pub async fn record(conn: &mut MySqlConnection, a: &Answered) -> AppResult<()> {
    // 1. Read model.
    let before = stat_from_row(mocks::stat_for_update(conn, a.user_id, a.question_id).await?);
    let after = before.apply(a.outcome, a.confidence, a.at);
    mocks::upsert_stat(conn, a.user_id, a.question_id, &stat_to_row(&after)).await?;

    // The answer as a user event: sessions, WeekSummary's first-try MC rate (spec 5.11).
    let qtype = bank::question_by_id(conn, a.question_id).await?.map(|q| q.qtype).unwrap_or_default();
    let (event_type, extra) = match a.source {
        Source::Mock { order, .. } => ("mock.graded", serde_json::json!({ "order": order })),
        Source::Attempt { .. } => ("attempt", serde_json::json!({})),
    };
    let mut payload = serde_json::json!({
        "questionType": qtype,
        "outcome": a.outcome.as_db(),
        "firstTry": before.attempts == 0,
        "confidence": a.confidence.map(|c| c.as_db()),
    });
    if let (Some(p), Some(x)) = (payload.as_object_mut(), extra.as_object()) {
        p.extend(x.clone());
    }
    crate::activity::server_event(
        conn,
        a.user_id,
        a.at,
        event_type,
        Some(a.course_id),
        Some(a.component_id),
        Some(("Question", a.question_id)),
        payload,
    )
    .await?;

    // 2. Slips on this question resolve when it's answered right (spec 5.6). A guess isn't proof.
    if a.outcome == Outcome::Right && a.confidence != Some(Confidence::Guess) {
        mistakes::resolve_slips_for_question(conn, a.user_id, a.question_id, a.at).await?;
    }

    // 3. Weak spot checks for the question's concepts.
    let concept_ids: Vec<i64> = bank::question_concepts(conn, &[a.question_id]).await?.into_iter().map(|(_, _, id, _)| id).collect();
    let spots = mistakes::open_spots_for_concepts(conn, a.user_id, a.course_id, &concept_ids).await?;
    if spots.is_empty() {
        return Ok(());
    }
    let cold = match a.cold {
        Some(c) => c,
        None => {
            let except = match a.source {
                Source::Mock { mock_id, .. } => Some(mock_id),
                Source::Attempt { .. } => None,
            };
            !mocks::studied_between(conn, a.user_id, a.component_id, a.at - Duration::hours(24), a.at, except).await?
        }
    };
    let passed = a.outcome == Outcome::Right && a.confidence != Some(Confidence::Guess);
    let check = Check { at: a.at, passed, cold };
    for spot in spots {
        let (attempt, mock) = match a.source {
            Source::Mock { mock_id, order } => (None, Some((mock_id, order))),
            Source::Attempt { attempt_id } => (Some(attempt_id), None),
        };
        mistakes::insert_check(conn, spot.id, a.at, passed, cold, attempt, mock).await?;
        let progress = Progress {
            status: WsStatus::from_db(&spot.status),
            severity: Severity::from_db(&spot.severity).unwrap_or(Severity::Medium),
            resolved_at: spot.resolved_at,
            misses_at_level: spot.misses_at_level.max(0) as u8,
            cold_run: spot.cold_pass_run.max(0) as u8,
        };
        let next = weakspot::apply(progress, spot.cold_passes_needed.max(1) as u8, &check);
        mistakes::set_weak_spot_state(
            conn,
            spot.id,
            next.status.as_db(),
            next.severity.as_db(),
            next.resolved_at,
            next.misses_at_level as i8,
            next.cold_run as i8,
            Some(a.at),
        )
        .await?;
    }
    Ok(())
}
