//! The WeakSpot state module (spec 2.3, rules 5.6) and the read-model / slip rules around it.
//! Open ⇄ Reopened → Resolved. Status and severity are a pure replay of the spot's checks.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};

use crate::grading::{Confidence, Outcome};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn as_db(self) -> &'static str {
        match self {
            Severity::Low => "Low",
            Severity::Medium => "Medium",
            Severity::High => "High",
            Severity::Critical => "Critical",
        }
    }

    pub fn from_db(s: &str) -> Option<Self> {
        Some(match s {
            "Low" => Severity::Low,
            "Medium" => Severity::Medium,
            "High" => Severity::High,
            "Critical" => Severity::Critical,
            _ => return None,
        })
    }

    fn up(self) -> Self {
        match self {
            Severity::Low => Severity::Medium,
            Severity::Medium => Severity::High,
            _ => Severity::Critical,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsStatus {
    Open,
    Resolved,
    Reopened,
}

impl WsStatus {
    pub fn as_db(self) -> &'static str {
        match self {
            WsStatus::Open => "Open",
            WsStatus::Resolved => "Resolved",
            WsStatus::Reopened => "Reopened",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Check {
    pub at: DateTime<Utc>,
    pub passed: bool,
    /// The spot's lecture wasn't studied in the previous 24 h.
    pub cold: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WsState {
    pub status: WsStatus,
    pub severity: Severity,
    pub resolved_at: Option<DateTime<Utc>>,
}

/// Rules 5.6:
/// - Resolved when the last `cold_passes_needed` cold checks all passed (a miss resets the run).
/// - A miss on a resolved spot reopens it.
/// - Severity goes up one level after 3 misses at the same level.
pub fn replay(initial: Severity, cold_passes_needed: u8, checks: &[Check]) -> WsState {
    let mut state = WsState { status: WsStatus::Open, severity: initial, resolved_at: None };
    let mut misses_at_level = 0;
    let mut cold_run = 0u8;
    let mut ordered = checks.to_vec();
    ordered.sort_by_key(|c| c.at);
    for c in ordered {
        if c.passed {
            if c.cold {
                cold_run = cold_run.saturating_add(1);
            }
            if cold_run >= cold_passes_needed.max(1) && state.status != WsStatus::Resolved {
                state.status = WsStatus::Resolved;
                state.resolved_at = Some(c.at);
            }
        } else {
            cold_run = 0;
            misses_at_level += 1;
            if misses_at_level >= 3 {
                state.severity = state.severity.up();
                misses_at_level = 0;
            }
            if state.status == WsStatus::Resolved {
                state.status = WsStatus::Reopened;
                state.resolved_at = None;
            }
        }
    }
    state
}

/// The OPEN line (spec 5.6): open spots by severity Critical → Low, then most recently seen.
pub fn open_line_order(a: (Severity, DateTime<Utc>), b: (Severity, DateTime<Utc>)) -> std::cmp::Ordering {
    b.0.cmp(&a.0).then(b.1.cmp(&a.1))
}

/// Spec 5.6: a weak spot is proposed when 2+ slips on one concept fall within 14 days.
pub const PROPOSAL_WINDOW: Duration = Duration::days(14);

pub fn concepts_needing_a_spot(slips: &[(i64, DateTime<Utc>)], now: DateTime<Utc>) -> Vec<(i64, usize)> {
    let mut counts: BTreeMap<i64, usize> = BTreeMap::new();
    for (concept, at) in slips {
        if now - *at <= PROPOSAL_WINDOW {
            *counts.entry(*concept).or_default() += 1;
        }
    }
    counts.into_iter().filter(|(_, n)| *n >= 2).collect()
}

/// Spec 5.4 "propose slips for wrong and sure-wrong answers".
pub fn deserves_slip(outcome: Outcome, _confidence: Option<Confidence>) -> bool {
    matches!(outcome, Outcome::Wrong | Outcome::Partial)
}

/// Spec 5.6: at most one source (mock answer, review or attempt). Review rule 1.
pub fn one_source(mock: bool, review: bool, attempt: bool) -> bool {
    [mock, review, attempt].iter().filter(|x| **x).count() <= 1
}

/// UserQuestionStat after one answer (spec 5.11: updated with each answer). A guessed Right
/// doesn't count as a Right (spec 5.3 counts guesses as misses; build choice).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stat {
    pub attempts: i32,
    pub rights: i32,
    pub sure_wrongs: i32,
    pub last_outcome: Option<Outcome>,
    pub last_confidence: Option<Confidence>,
    pub last_answered_at: Option<DateTime<Utc>>,
    pub first_right_at: Option<DateTime<Utc>>,
}

impl Stat {
    pub const EMPTY: Stat =
        Stat { attempts: 0, rights: 0, sure_wrongs: 0, last_outcome: None, last_confidence: None, last_answered_at: None, first_right_at: None };

    pub fn apply(self, outcome: Outcome, confidence: Option<Confidence>, at: DateTime<Utc>) -> Stat {
        let solid_right = outcome == Outcome::Right && confidence != Some(Confidence::Guess);
        let newer = self.last_answered_at.is_none_or(|l| at >= l);
        Stat {
            attempts: self.attempts + 1,
            rights: self.rights + i32::from(solid_right),
            sure_wrongs: self.sure_wrongs + i32::from(outcome == Outcome::Wrong && confidence == Some(Confidence::Sure)),
            last_outcome: if newer { Some(outcome) } else { self.last_outcome },
            last_confidence: if newer { confidence } else { self.last_confidence },
            last_answered_at: if newer { Some(at) } else { self.last_answered_at },
            first_right_at: match (self.first_right_at, solid_right) {
                (Some(f), _) => Some(f.min(if solid_right { at } else { f })),
                (None, true) => Some(at),
                (None, false) => None,
            },
        }
    }

    /// Spec 5.3 step 5: latest outcome Wrong, Blank or a guess.
    pub fn last_was_miss(&self) -> bool {
        self.last_outcome.is_some_and(|o| o.is_miss()) || self.last_confidence == Some(Confidence::Guess)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap() + Duration::hours(h)
    }

    fn chk(h: i64, passed: bool, cold: bool) -> Check {
        Check { at: at(h), passed, cold }
    }

    #[test]
    fn resolves_after_enough_cold_passes_and_reopens_on_a_miss() {
        let s = replay(Severity::Medium, 2, &[chk(0, true, true), chk(30, true, false)]);
        assert_eq!(s.status, WsStatus::Open, "a warm pass doesn't count");
        let s = replay(Severity::Medium, 2, &[chk(0, true, true), chk(30, true, true)]);
        assert_eq!((s.status, s.resolved_at), (WsStatus::Resolved, Some(at(30))));
        let s = replay(Severity::Medium, 2, &[chk(0, true, true), chk(30, true, true), chk(60, false, false)]);
        assert_eq!(s.status, WsStatus::Reopened);
        let s = replay(Severity::Medium, 2, &[chk(0, true, true), chk(30, false, true), chk(60, true, true)]);
        assert_eq!(s.status, WsStatus::Open, "a miss resets the cold run");
    }

    #[test]
    fn severity_rises_after_three_misses_at_a_level() {
        let misses: Vec<Check> = (0..3).map(|h| chk(h, false, false)).collect();
        assert_eq!(replay(Severity::Low, 2, &misses).severity, Severity::Medium);
        let six: Vec<Check> = (0..6).map(|h| chk(h, false, false)).collect();
        assert_eq!(replay(Severity::Low, 2, &six).severity, Severity::High);
        let many: Vec<Check> = (0..20).map(|h| chk(h, false, false)).collect();
        assert_eq!(replay(Severity::High, 2, &many).severity, Severity::Critical);
    }

    #[test]
    fn checks_replay_in_time_order() {
        let s = replay(Severity::Low, 1, &[chk(5, false, false), chk(1, true, true)]);
        assert_eq!(s.status, WsStatus::Reopened, "resolved at hour 1, reopened by the later miss");
    }

    #[test]
    fn proposals_and_open_line() {
        let now = at(24 * 20);
        let slips = vec![(1, at(24 * 10)), (1, at(24 * 19)), (2, at(0)), (2, at(24 * 19)), (3, at(24 * 19))];
        assert_eq!(concepts_needing_a_spot(&slips, now), vec![(1, 2)], "concept 2's first slip is too old");
        assert_eq!(open_line_order((Severity::High, at(0)), (Severity::Critical, at(0))), std::cmp::Ordering::Greater);
    }

    #[test]
    fn sources_and_stats() {
        assert!(one_source(true, false, false) && !one_source(true, true, false));
        let s = Stat::EMPTY.apply(Outcome::Right, Some(Confidence::Guess), at(1));
        assert_eq!((s.rights, s.first_right_at), (0, None), "a guess isn't a right");
        assert!(s.last_was_miss());
        let s = s.apply(Outcome::Wrong, Some(Confidence::Sure), at(2)).apply(Outcome::Right, Some(Confidence::Sure), at(3));
        assert_eq!((s.attempts, s.rights, s.sure_wrongs, s.first_right_at), (3, 1, 1, Some(at(3))));
        assert!(!s.last_was_miss());
        let late = s.apply(Outcome::Wrong, None, at(0));
        assert_eq!(late.last_outcome, Some(Outcome::Right), "an older answer arriving late doesn't become 'last'");
    }
}
