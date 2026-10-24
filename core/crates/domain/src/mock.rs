//! The MockExam state module (spec 2.3): the only code that decides mock and answer transitions.
//! Draft → InProgress → Submitted → (Grading) → Graded → Reviewed.

use chrono::{DateTime, Duration, Utc};

use crate::grading::Outcome;

/// Spec 5.4: a server-side time limit with a 2-minute grace period.
pub const GRACE: Duration = Duration::minutes(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockStatus {
    Draft,
    InProgress,
    Submitted,
    Grading,
    Graded,
    Reviewed,
}

impl MockStatus {
    pub fn as_db(self) -> &'static str {
        match self {
            MockStatus::Draft => "Draft",
            MockStatus::InProgress => "InProgress",
            MockStatus::Submitted => "Submitted",
            MockStatus::Grading => "Grading",
            MockStatus::Graded => "Graded",
            MockStatus::Reviewed => "Reviewed",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "InProgress" => MockStatus::InProgress,
            "Submitted" => MockStatus::Submitted,
            "Grading" => MockStatus::Grading,
            "Graded" => MockStatus::Graded,
            "Reviewed" => MockStatus::Reviewed,
            _ => MockStatus::Draft,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MockError {
    #[error("the mock isn't in a state that allows this")]
    WrongState,
    #[error("time is over")]
    TimeOver,
    #[error("this item has no grade to give")]
    NotGradable,
}

pub fn transition(from: MockStatus, to: MockStatus) -> Result<MockStatus, MockError> {
    use MockStatus::*;
    let ok = matches!(
        (from, to),
        (Draft, InProgress) | (InProgress, Submitted) | (Submitted, Grading) | (Submitted, Graded) | (Grading, Graded) | (Graded, Reviewed)
    );
    if ok { Ok(to) } else { Err(MockError::WrongState) }
}

/// When answers stop being accepted: start + limit + grace. None = untimed.
pub fn hard_deadline(started_at: DateTime<Utc>, time_limit_min: Option<i32>) -> Option<DateTime<Utc>> {
    time_limit_min.map(|m| started_at + Duration::minutes(m.into()) + GRACE)
}

/// Answers are accepted while InProgress and before the hard deadline.
pub fn may_answer(status: MockStatus, started_at: Option<DateTime<Utc>>, limit: Option<i32>, now: DateTime<Utc>) -> Result<(), MockError> {
    if status != MockStatus::InProgress {
        return Err(MockError::WrongState);
    }
    match started_at.and_then(|s| hard_deadline(s, limit)) {
        Some(end) if now > end => Err(MockError::TimeOver),
        _ => Ok(()),
    }
}

/// What submit leaves behind: Graded when nothing needs a human (or AI) grader, else Submitted
/// (self-grading pending) or Grading (AI suggestions pending; spec 5.4).
pub fn status_after_submit(ungraded_open_items: usize, ai_mode: bool) -> MockStatus {
    match (ungraded_open_items, ai_mode) {
        (0, _) => MockStatus::Graded,
        (_, true) => MockStatus::Grading,
        (_, false) => MockStatus::Submitted,
    }
}

/// One item as the review rules see it.
#[derive(Debug, Clone, Copy)]
pub struct ItemReview {
    pub outcome: Option<Outcome>,
    pub revealed: bool,
    pub self_shown: bool,
    pub fixed: bool,
}

/// Spec 5.4: the key, explanation and model answer show only for items Revealed or SelfShown, or
/// when RevealAll. (Right items are marked Revealed at grading: nothing to hide; build choice.)
pub fn key_visible(item: &ItemReview, reveal_all: bool) -> bool {
    reveal_all || item.revealed || item.self_shown
}

/// Spec 5.4: when every miss is fixed or shown, RevealAll and Reviewed. A miss here is anything
/// short of Right (Partial included).
pub fn review_complete(items: &[ItemReview]) -> bool {
    items.iter().filter(|i| i.outcome != Some(Outcome::Right)).all(|i| i.fixed || i.self_shown)
}

/// Spec 5.4: score is correct/total with Partial = 0.5, i.e. Σ grades over Σ max grades.
pub fn score(grades: &[(Option<f64>, f64)]) -> (f64, f64) {
    let got: f64 = grades.iter().filter_map(|(g, _)| *g).sum();
    let max: f64 = grades.iter().map(|(_, m)| m).sum();
    ((got * 100.0).round() / 100.0, (max * 100.0).round() / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use MockStatus::*;

    fn t(min: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 24, 9, 0, 0).unwrap() + Duration::minutes(min)
    }

    #[test]
    fn transitions() {
        assert_eq!(transition(Draft, InProgress), Ok(InProgress));
        assert_eq!(transition(Submitted, Graded), Ok(Graded));
        assert_eq!(transition(Grading, Graded), Ok(Graded));
        for (a, b) in [(Draft, Submitted), (InProgress, Graded), (Graded, InProgress), (Reviewed, Graded), (Grading, Reviewed)] {
            assert_eq!(transition(a, b), Err(MockError::WrongState), "{a:?} -> {b:?}");
        }
    }

    #[test]
    fn time_limit_with_grace() {
        assert_eq!(may_answer(InProgress, Some(t(0)), Some(60), t(61)), Ok(()), "inside the 2-minute grace");
        assert_eq!(may_answer(InProgress, Some(t(0)), Some(60), t(62)), Ok(()));
        assert_eq!(may_answer(InProgress, Some(t(0)), Some(60), t(63)), Err(MockError::TimeOver));
        assert_eq!(may_answer(InProgress, Some(t(0)), None, t(10_000)), Ok(()), "untimed");
        assert_eq!(may_answer(Submitted, Some(t(0)), None, t(1)), Err(MockError::WrongState));
    }

    #[test]
    fn submit_outcome() {
        assert_eq!(status_after_submit(0, true), Graded);
        assert_eq!(status_after_submit(2, false), Submitted);
        assert_eq!(status_after_submit(2, true), Grading);
    }

    #[test]
    fn reveal_and_review() {
        let right = ItemReview { outcome: Some(Outcome::Right), revealed: true, self_shown: false, fixed: false };
        let wrong = ItemReview { outcome: Some(Outcome::Wrong), revealed: false, self_shown: false, fixed: false };
        assert!(key_visible(&right, false));
        assert!(!key_visible(&wrong, false));
        assert!(key_visible(&wrong, true));
        assert!(!review_complete(&[right, wrong]));
        assert!(review_complete(&[right, ItemReview { fixed: true, ..wrong }]));
        assert!(review_complete(&[right, ItemReview { self_shown: true, ..wrong }]));
        let partial = ItemReview { outcome: Some(Outcome::Partial), ..wrong };
        assert!(!review_complete(&[partial]), "partial counts as a miss in review");
    }

    #[test]
    fn scores() {
        assert_eq!(score(&[(Some(1.0), 1.0), (Some(0.5), 1.0), (Some(0.0), 1.0), (None, 1.0)]), (1.5, 4.0));
    }
}
