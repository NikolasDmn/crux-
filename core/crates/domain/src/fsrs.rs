//! The FlashcardProgress state module (spec 2.3, 5.5): New → Learning → Review ⇄ Relearning.
//!
//! Memory (stability, difficulty) and the review interval come from the `fsrs` crate, the
//! open-spaced-repetition reference implementation, with its default parameters (FSRS-6 since
//! crate 6.x; the spec says FSRS-5, see OPEN-QUESTIONS B4-1). The crate gives day intervals only,
//! so short learning steps are added here, Anki-style.

use std::sync::OnceLock;

use chrono::{DateTime, Duration, NaiveTime, Utc};
use chrono_tz::Tz;

/// Stored on every card the scheduler touches (`FlashcardProgress.FsrsParamsVersion`) so cards can
/// be refit later from the review log.
pub const PARAMS_VERSION: &str = "fsrs6.6-dflt";

/// Learning steps (not part of FSRS): a failed or hard new card comes back within minutes.
pub const LEARN_AGAIN: Duration = Duration::minutes(1);
pub const LEARN_HARD: Duration = Duration::minutes(6);
pub const RELEARN: Duration = Duration::minutes(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardState {
    New,
    Learning,
    Review,
    Relearning,
}

impl CardState {
    pub fn as_db(self) -> &'static str {
        match self {
            CardState::New => "New",
            CardState::Learning => "Learning",
            CardState::Review => "Review",
            CardState::Relearning => "Relearning",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "Learning" => CardState::Learning,
            "Review" => CardState::Review,
            "Relearning" => CardState::Relearning,
            _ => CardState::New,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Card {
    pub state: CardState,
    pub stability: Option<f64>,
    pub difficulty: Option<f64>,
    pub due_at: Option<DateTime<Utc>>,
    pub last_reviewed_at: Option<DateTime<Utc>>,
    pub reps: i32,
    pub lapses: i32,
}

impl Card {
    pub const NEW: Card =
        Card { state: CardState::New, stability: None, difficulty: None, due_at: None, last_reviewed_at: None, reps: 0, lapses: 0 };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReviewError {
    #[error("rating must be 1-4")]
    BadRating,
    #[error("scheduler error")]
    Scheduler,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reviewed {
    pub card: Card,
    /// The review predates the card's last review (an offline review synced late): it's logged
    /// but doesn't reschedule (spec 5.5).
    pub stale: bool,
    /// Again on a card in Review: a lapse (becomes a slip proposal, spec 5.5).
    pub lapsed: bool,
}

fn scheduler() -> &'static fsrs::FSRS {
    static S: OnceLock<fsrs::FSRS> = OnceLock::new();
    S.get_or_init(fsrs::FSRS::default)
}

/// Spec 5.5 exam cap: a new due date never lands after the course's next exam; if it would, it
/// becomes 18:00 local the day before the exam (or now, if that has passed already).
pub fn exam_cap(due: DateTime<Utc>, next_exam: Option<DateTime<Utc>>, tz: Tz, now: DateTime<Utc>) -> DateTime<Utc> {
    let Some(exam) = next_exam else { return due };
    if due <= exam {
        return due;
    }
    let day_before = exam.with_timezone(&tz).date_naive() - Duration::days(1);
    let evening = crate::localtime::to_utc(tz, day_before.and_time(NaiveTime::from_hms_opt(18, 0, 0).expect("18:00")));
    evening.max(now).min(exam)
}

/// One rating (1 Again, 2 Hard, 3 Good, 4 Easy) at `at`.
pub fn review(
    card: &Card,
    rating: u8,
    at: DateTime<Utc>,
    retention: f64,
    tz: Tz,
    next_exam: Option<DateTime<Utc>>,
) -> Result<Reviewed, ReviewError> {
    if !(1..=4).contains(&rating) {
        return Err(ReviewError::BadRating);
    }
    if card.last_reviewed_at.is_some_and(|last| at < last) {
        return Ok(Reviewed { card: *card, stale: true, lapsed: false });
    }
    let memory = match (card.stability, card.difficulty) {
        (Some(s), Some(d)) if card.state != CardState::New => Some(fsrs::MemoryState { stability: s as f32, difficulty: d as f32 }),
        _ => None,
    };
    let elapsed_days = card.last_reviewed_at.map(|l| (at - l).num_days().max(0) as u32).unwrap_or(0);
    let next = scheduler().next_states(memory, retention as f32, elapsed_days).map_err(|_| ReviewError::Scheduler)?;
    let item = match rating {
        1 => next.again,
        2 => next.hard,
        3 => next.good,
        _ => next.easy,
    };
    let days = |interval: f32| Duration::seconds(((interval.max(1.0) as f64) * 86_400.0).round() as i64);
    use CardState::*;
    let (state, due, lapsed) = match (card.state, rating) {
        (New | Learning, 1) => (Learning, at + LEARN_AGAIN, false),
        (New | Learning, 2) => (Learning, at + LEARN_HARD, false),
        (New | Learning, _) => (Review, at + days(item.interval), false),
        (Review, 1) => (Relearning, at + RELEARN, true),
        (Relearning, 1) => (Relearning, at + RELEARN, false),
        (Review | Relearning, _) => (Review, at + days(item.interval), false),
    };
    let due = exam_cap(due, next_exam, tz, at);
    Ok(Reviewed {
        card: Card {
            state,
            stability: Some(item.memory.stability as f64),
            difficulty: Some((item.memory.difficulty as f64).clamp(1.0, 10.0)),
            due_at: Some(due),
            last_reviewed_at: Some(at),
            reps: card.reps + 1,
            lapses: card.lapses + i32::from(lapsed),
        },
        stale: false,
        lapsed,
    })
}

/// Spec 5.5 codes: `<COURSE CODE><3+ digits>`, the next number per course, never renumbered.
pub fn next_code(course_code: &str, existing: &[String]) -> String {
    let max = existing
        .iter()
        .filter_map(|c| c.strip_prefix(course_code))
        .filter(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
        .filter_map(|rest| rest.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("{course_code}{:03}", max + 1)
}

/// Spec 5.5 queue: new cards today = min(FlashNewPerDay − new already seen today, available).
pub fn new_cards_left(new_per_day: i32, new_seen_today: i64) -> i64 {
    (i64::from(new_per_day) - new_seen_today).max(0)
}

/// Start of "today" in the user's time zone, in UTC.
pub fn local_day_start(now: DateTime<Utc>, tz: Tz) -> DateTime<Utc> {
    let date = now.with_timezone(&tz).date_naive();
    crate::localtime::to_utc(tz, date.and_time(NaiveTime::MIN))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const AMS: Tz = chrono_tz::Europe::Amsterdam;

    fn t(day: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, h, 0, 0).unwrap()
    }

    #[test]
    fn new_card_paths() {
        let again = review(&Card::NEW, 1, t(1, 9), 0.9, AMS, None).unwrap().card;
        assert_eq!((again.state, again.due_at), (CardState::Learning, Some(t(1, 9) + LEARN_AGAIN)));
        let good = review(&Card::NEW, 3, t(1, 9), 0.9, AMS, None).unwrap().card;
        assert_eq!(good.state, CardState::Review);
        assert!(good.due_at.unwrap() >= t(2, 9), "at least a day");
        let easy = review(&Card::NEW, 4, t(1, 9), 0.9, AMS, None).unwrap().card;
        assert!(easy.due_at > good.due_at, "easy is spaced further than good");
        assert!(easy.stability > good.stability);
        assert_eq!(good.reps, 1);
    }

    #[test]
    fn lapse_and_relearn() {
        let good = review(&Card::NEW, 3, t(1, 9), 0.9, AMS, None).unwrap().card;
        let r = review(&good, 1, t(5, 9), 0.9, AMS, None).unwrap();
        assert!(r.lapsed);
        assert_eq!((r.card.state, r.card.lapses, r.card.due_at), (CardState::Relearning, 1, Some(t(5, 9) + RELEARN)));
        let back = review(&r.card, 3, t(5, 10), 0.9, AMS, None).unwrap().card;
        assert_eq!(back.state, CardState::Review);
    }

    #[test]
    fn stale_reviews_dont_reschedule() {
        let good = review(&Card::NEW, 3, t(5, 9), 0.9, AMS, None).unwrap().card;
        let late = review(&good, 1, t(4, 9), 0.9, AMS, None).unwrap();
        assert!(late.stale);
        assert_eq!(late.card, good);
        assert_eq!(review(&good, 5, t(6, 9), 0.9, AMS, None), Err(ReviewError::BadRating));
    }

    #[test]
    fn higher_retention_means_shorter_intervals() {
        let lax = review(&Card::NEW, 3, t(1, 9), 0.8, AMS, None).unwrap().card;
        let strict = review(&Card::NEW, 3, t(1, 9), 0.97, AMS, None).unwrap().card;
        assert!(strict.due_at < lax.due_at);
    }

    #[test]
    fn exam_cap_moves_due_to_the_evening_before() {
        let exam = t(20, 8); // 10:00 Amsterdam (CEST, UTC+2)
        assert_eq!(exam_cap(t(15, 9), Some(exam), AMS, t(1, 9)), t(15, 9), "before the exam: unchanged");
        assert_eq!(exam_cap(t(25, 9), Some(exam), AMS, t(1, 9)), t(19, 16), "18:00 Amsterdam on the 19th = 16:00 UTC");
        assert_eq!(exam_cap(t(25, 9), Some(exam), AMS, t(19, 20)), t(19, 20), "already past 18:00 the day before: now");
        let easy = review(&Card::NEW, 4, t(18, 9), 0.9, AMS, Some(exam)).unwrap().card;
        assert!(easy.due_at.unwrap() <= t(19, 16));
    }

    #[test]
    fn codes_and_limits() {
        assert_eq!(next_code("BSA", &[]), "BSA001");
        assert_eq!(next_code("BSA", &["BSA001".into(), "BSA017".into(), "BSAX9".into(), "RK999".into()]), "BSA018");
        assert_eq!(next_code("BSA", &["BSA999".into()]), "BSA1000", "3+ digits");
        assert_eq!(new_cards_left(20, 5), 15);
        assert_eq!(new_cards_left(20, 25), 0);
        assert_eq!(local_day_start(t(10, 1), AMS), t(9, 22), "01:00 UTC is already the 10th in Amsterdam");
    }
}
