//! Study time (backend-spec 5.7): derived sessions from user events, per-lecture attribution and
//! the remaining-hours estimate.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};

/// A gap of this long or more starts a new session.
pub const SESSION_GAP: Duration = Duration::minutes(30);
/// A session runs to the last event plus this.
pub const SESSION_TAIL: Duration = Duration::minutes(2);
/// `StudySession` refuses sessions of 24 h or more (schema CHECK).
pub const MAX_SESSION: Duration = Duration::hours(23);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SessionKind {
    Lecture,
    Practice,
    Mock,
    Flashcards,
    Review,
    AiChat,
    Other,
}

impl SessionKind {
    pub fn as_db(self) -> &'static str {
        match self {
            SessionKind::Lecture => "Lecture",
            SessionKind::Practice => "Practice",
            SessionKind::Mock => "Mock",
            SessionKind::Flashcards => "Flashcards",
            SessionKind::Review => "Review",
            SessionKind::AiChat => "AiChat",
            SessionKind::Other => "Other",
        }
    }

    pub fn from_db(s: &str) -> Option<Self> {
        Some(match s {
            "Lecture" => SessionKind::Lecture,
            "Practice" => SessionKind::Practice,
            "Mock" => SessionKind::Mock,
            "Flashcards" => SessionKind::Flashcards,
            "Review" => SessionKind::Review,
            "AiChat" => SessionKind::AiChat,
            "Other" => SessionKind::Other,
            _ => return None,
        })
    }

    /// What kind of studying an event type stands for.
    pub fn of_event(event_type: &str) -> Self {
        let family = event_type.split('.').next().unwrap_or("");
        match family {
            "material" | "notes" | "lecture" => SessionKind::Lecture,
            "attempt" | "warmup" => SessionKind::Practice,
            "mock" => SessionKind::Mock,
            "flash" => SessionKind::Flashcards,
            "review" => SessionKind::Review,
            "ai" => SessionKind::AiChat,
            _ => SessionKind::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ev {
    pub at: DateTime<Utc>,
    pub course_id: i64,
    pub component_id: Option<i64>,
    pub kind: SessionKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Derived {
    pub course_id: i64,
    pub component_id: Option<i64>,
    pub kind: SessionKind,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
}

fn majority<K: Ord + Copy>(items: impl Iterator<Item = K>) -> Option<K> {
    let mut counts: BTreeMap<K, usize> = BTreeMap::new();
    for k in items {
        *counts.entry(k).or_default() += 1;
    }
    // Ties go to the smallest key, so the result doesn't depend on event order.
    counts
        .into_iter()
        .fold(None, |best: Option<(K, usize)>, (k, n)| match best {
            Some((_, bn)) if bn >= n => best,
            _ => Some((k, n)),
        })
        .map(|(k, _)| k)
}

/// Spec 5.7: per course, a gap of 30 min or more starts a new session; a session runs from the
/// first event to the last + 2 min; it's attributed to the lecture (and kind) with most events.
/// `events` may be in any order and mix courses.
pub fn derive(events: &[Ev]) -> Vec<Derived> {
    let mut by_course: BTreeMap<i64, Vec<&Ev>> = BTreeMap::new();
    for e in events {
        by_course.entry(e.course_id).or_default().push(e);
    }
    let mut out = Vec::new();
    for (course_id, mut evs) in by_course {
        evs.sort_by_key(|e| e.at);
        let mut group: Vec<&Ev> = Vec::new();
        let flush = |group: &mut Vec<&Ev>, out: &mut Vec<Derived>| {
            let (Some(first), Some(last)) = (group.first(), group.last()) else { return };
            let started_at = first.at;
            let ended_at = (last.at + SESSION_TAIL).min(started_at + MAX_SESSION);
            out.push(Derived {
                course_id,
                component_id: majority(group.iter().filter_map(|e| e.component_id)),
                kind: majority(group.iter().map(|e| e.kind)).unwrap_or(SessionKind::Other),
                started_at,
                ended_at,
            });
            group.clear();
        };
        for e in evs {
            if let Some(prev) = group.last() {
                if e.at - prev.at >= SESSION_GAP || e.at - group[0].at >= MAX_SESSION {
                    flush(&mut group, &mut out);
                }
            }
            group.push(e);
        }
        flush(&mut group, &mut out);
    }
    out.sort_by_key(|d| (d.started_at, d.course_id));
    out
}

/// Timer and manual sessions win over derived ones that overlap them (spec 5.7).
pub fn drop_overlapping(derived: Vec<Derived>, explicit: &[(DateTime<Utc>, DateTime<Utc>)]) -> Vec<Derived> {
    derived.into_iter().filter(|d| !explicit.iter().any(|(s, e)| d.started_at < *e && *s < d.ended_at)).collect()
}

/// Remaining hours as a range (spec 5.7).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Remaining {
    pub low: f64,
    pub mid: f64,
    pub high: f64,
    /// actual / estimate on the finished lectures, once 3 are done.
    pub ratio: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
pub struct LectureTime {
    pub est_hours: Option<f64>,
    pub actual_hours: f64,
    pub done: bool,
}

/// Σ estimated hours of lectures that aren't done: ±25% until 3 lectures are done; after that
/// scaled by the actual/estimate ratio of the finished ones, ±10% (build choice B5-6).
pub fn remaining(lectures: &[LectureTime]) -> Remaining {
    let left: f64 = lectures.iter().filter(|l| !l.done).filter_map(|l| l.est_hours).sum();
    let done: Vec<&LectureTime> = lectures.iter().filter(|l| l.done && l.est_hours.is_some_and(|h| h > 0.0)).collect();
    if done.len() < 3 {
        return Remaining { low: left * 0.75, mid: left, high: left * 1.25, ratio: None };
    }
    let est: f64 = done.iter().filter_map(|l| l.est_hours).sum();
    let actual: f64 = done.iter().map(|l| l.actual_hours).sum();
    let ratio = if est > 0.0 { (actual / est).clamp(0.25, 4.0) } else { 1.0 };
    let mid = left * ratio;
    Remaining { low: mid * 0.9, mid, high: mid * 1.1, ratio: Some(ratio) }
}

/// Passing a graded exam (spec 5.10): the deadline's PassMark wins over the scale's PassValue.
pub fn passed(grade: Option<f64>, pass_mark: Option<f64>, scale_pass_value: Option<f64>, pass_fail_text: Option<&str>) -> Option<bool> {
    if let Some(t) = pass_fail_text {
        match t.trim().to_ascii_lowercase().as_str() {
            "pass" | "passed" | "p" => return Some(true),
            "fail" | "failed" | "f" => return Some(false),
            _ => {}
        }
    }
    let g = grade?;
    let threshold = pass_mark.or(scale_pass_value)?;
    Some(g >= threshold)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 20, h, m, 0).unwrap()
    }

    fn ev(h: u32, m: u32, course: i64, comp: Option<i64>, t: &str) -> Ev {
        Ev { at: at(h, m), course_id: course, component_id: comp, kind: SessionKind::of_event(t) }
    }

    #[test]
    fn gaps_split_sessions_and_tail_is_added() {
        let s = derive(&[
            ev(9, 0, 1, Some(10), "material.open"),
            ev(9, 20, 1, Some(10), "material.page"),
            ev(9, 49, 1, Some(11), "mock.answer"),  // 29 min gap: same session
            ev(10, 19, 1, Some(11), "mock.answer"), // 30 min gap: new session
        ]);
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].started_at, s[0].ended_at), (at(9, 0), at(9, 51)));
        assert_eq!((s[0].component_id, s[0].kind), (Some(10), SessionKind::Lecture), "majority lecture and kind");
        assert_eq!((s[1].started_at, s[1].ended_at, s[1].kind), (at(10, 19), at(10, 21), SessionKind::Mock));
    }

    #[test]
    fn courses_are_separate_and_order_doesnt_matter() {
        let s = derive(&[ev(9, 10, 2, None, "flash.review"), ev(9, 0, 1, None, "attempt"), ev(9, 5, 2, None, "flash.review")]);
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].course_id, s[0].kind), (1, SessionKind::Practice));
        assert_eq!((s[1].course_id, s[1].started_at, s[1].ended_at), (2, at(9, 5), at(9, 12)));
    }

    #[test]
    fn explicit_sessions_win() {
        let s = derive(&[ev(9, 0, 1, None, "attempt"), ev(12, 0, 1, None, "attempt")]);
        let kept = drop_overlapping(s, &[(at(8, 30), at(9, 1))]);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].started_at, at(12, 0));
    }

    #[test]
    fn remaining_range() {
        let l = |est: f64, actual: f64, done: bool| LectureTime { est_hours: Some(est), actual_hours: actual, done };
        let r = remaining(&[l(2.0, 3.0, true), l(2.0, 0.0, false), l(4.0, 0.0, false)]);
        assert_eq!((r.low, r.mid, r.high, r.ratio), (4.5, 6.0, 7.5, None));
        let r = remaining(&[l(2.0, 3.0, true), l(2.0, 3.0, true), l(2.0, 3.0, true), l(4.0, 0.0, false)]);
        assert_eq!(r.ratio, Some(1.5));
        assert!((r.mid - 6.0).abs() < 1e-9 && (r.low - 5.4).abs() < 1e-9);
    }

    #[test]
    fn pass_rules() {
        assert_eq!(passed(Some(5.5), None, Some(5.5), None), Some(true));
        assert_eq!(passed(Some(5.4), None, Some(5.5), None), Some(false));
        assert_eq!(passed(Some(6.0), Some(6.5), Some(5.5), None), Some(false), "PassMark wins");
        assert_eq!(passed(None, None, None, Some("Pass")), Some(true));
        assert_eq!(passed(None, None, Some(5.5), None), None);
    }
}
