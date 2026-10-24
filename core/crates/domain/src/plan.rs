//! The auto plan (backend-spec 5.8). Pure: the service passes in courses, remaining lecture hours,
//! exams, pinned entries and the user's settings, and stores what comes back.
//!
//! Rules:
//! - courses ordered by deadline first, then remaining hours ÷ days left;
//! - the first pass of all lectures before the final week (the planner works front to back, so
//!   lectures land as early as capacity allows; whatever still lands later is reported short);
//! - the last 2 days before an exam: revision + a timed mock (the mock on the first of the two);
//! - at most one course per day unless `multi_course`;
//! - `daily_hours` of capacity per day, minus pinned entries that day.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;

pub const DEFAULT_DAILY_HOURS: f64 = 4.0;
/// Planning horizon when no course has an exam.
pub const HORIZON_NO_EXAM_DAYS: i64 = 14;
pub const MAX_HORIZON_DAYS: i64 = 120;
/// The default study window (local time); users set their own (`Extra.studyStart` / `studyEnd`).
pub const DAY_START: (u32, u32) = (9, 0);
pub const DAY_END: (u32, u32) = (22, 0);
/// The shortest study window a user may set, in minutes.
pub const MIN_WINDOW_MINUTES: u32 = 60;
const MIN_BLOCK_HOURS: f64 = 0.5;
const FINAL_WEEK_DAYS: i64 = 7;
const DEFAULT_MOCK_MINUTES: i64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntryKind {
    Lecture,
    Revision,
    Mock,
    Other,
}

impl EntryKind {
    pub fn as_db(self) -> &'static str {
        match self {
            EntryKind::Lecture => "Lecture",
            EntryKind::Revision => "Revision",
            EntryKind::Mock => "Mock",
            EntryKind::Other => "Other",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "Lecture" => EntryKind::Lecture,
            "Revision" => EntryKind::Revision,
            "Mock" => EntryKind::Mock,
            _ => EntryKind::Other,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlanLecture {
    pub component_id: i64,
    pub code: String,
    pub name: String,
    pub remaining_hours: f64,
}

#[derive(Debug, Clone)]
pub struct PlanCourse {
    pub course_id: i64,
    pub code: String,
    pub exam: Option<DateTime<Utc>>,
    pub exam_minutes: Option<i32>,
    /// Unfinished lectures in course order.
    pub lectures: Vec<PlanLecture>,
}

#[derive(Debug, Clone)]
pub struct PlanInput {
    pub now: DateTime<Utc>,
    pub tz: Tz,
    pub daily_hours: f64,
    pub multi_course: bool,
    /// The day's study window, local `(hour, minute)`.
    pub day_start: (u32, u32),
    pub day_end: (u32, u32),
    pub courses: Vec<PlanCourse>,
    /// Pinned (user) entries from now on: their time is taken.
    pub busy: Vec<(DateTime<Utc>, DateTime<Utc>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub course_id: i64,
    pub component_id: Option<i64>,
    pub kind: EntryKind,
    pub description: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub entries: Vec<Entry>,
    /// Courses whose lectures don't fit before the final week: (course, hours that landed late or
    /// not at all).
    pub short: Vec<(i64, f64)>,
}

/// `"HH:MM"` on a 15-minute step, e.g. `"08:30"`.
pub fn parse_hhmm(s: &str) -> Option<(u32, u32)> {
    let (h, m) = s.split_once(':')?;
    if h.len() != 2 || m.len() != 2 {
        return None;
    }
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60 && m % 15 == 0).then_some((h, m))
}

fn local(tz: Tz, d: NaiveDate, (h, m): (u32, u32)) -> DateTime<Utc> {
    let t = NaiveTime::from_hms_opt(h, m, 0).expect("valid time");
    tz.from_local_datetime(&d.and_time(t))
        .earliest()
        .map(|x| x.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&d.and_time(t)))
}

fn hours(d: Duration) -> f64 {
    d.num_minutes() as f64 / 60.0
}

fn round_up_15(t: DateTime<Utc>) -> DateTime<Utc> {
    let secs = t.timestamp();
    let q = 15 * 60;
    let up = (secs + q - 1) / q * q;
    DateTime::from_timestamp(up, 0).unwrap_or(t)
}

/// Places blocks one after another in a day, stepping over busy intervals.
struct DayCursor<'a> {
    at: DateTime<Utc>,
    end: DateTime<Utc>,
    busy: &'a [(DateTime<Utc>, DateTime<Utc>)],
}

impl DayCursor<'_> {
    /// Up to `want` hours from the cursor, in one contiguous block; None when the day is full.
    fn take(&mut self, want: f64) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        loop {
            if self.at >= self.end {
                return None;
            }
            if let Some(b) = self.busy.iter().find(|(s, e)| *s <= self.at && self.at < *e) {
                self.at = b.1;
                continue;
            }
            let next_busy = self.busy.iter().filter(|(s, _)| *s > self.at).map(|(s, _)| *s).min();
            let limit = next_busy.map_or(self.end, |b| b.min(self.end));
            let free = hours(limit - self.at);
            if free < MIN_BLOCK_HOURS.min(want) - 1e-9 {
                self.at = limit;
                continue;
            }
            let len = want.min(free);
            let start = self.at;
            let end = start + Duration::minutes((len * 60.0).round() as i64);
            self.at = end;
            return Some((start, end));
        }
    }
}

pub fn build(input: &PlanInput) -> Plan {
    let tz = input.tz;
    let today = input.now.with_timezone(&tz).date_naive();
    let exam_day = |c: &PlanCourse| c.exam.map(|e| e.with_timezone(&tz).date_naive());
    let last_exam = input.courses.iter().filter_map(exam_day).filter(|d| *d > today).max();
    let horizon_end = last_exam.map(|d| d - Duration::days(1)).unwrap_or(today + Duration::days(HORIZON_NO_EXAM_DAYS - 1));
    let horizon_end = horizon_end.min(today + Duration::days(MAX_HORIZON_DAYS));

    // Order: deadline first, then remaining hours ÷ days left (more pressure first).
    let mut order: Vec<&PlanCourse> = input.courses.iter().collect();
    let pressure = |c: &PlanCourse| {
        let left: f64 = c.lectures.iter().map(|l| l.remaining_hours).sum();
        let days = exam_day(c).map(|d| (d - today).num_days().max(1)).unwrap_or(HORIZON_NO_EXAM_DAYS) as f64;
        left / days
    };
    order.sort_by(|a, b| {
        let (da, db) = (exam_day(a).unwrap_or(NaiveDate::MAX), exam_day(b).unwrap_or(NaiveDate::MAX));
        da.cmp(&db).then(pressure(b).total_cmp(&pressure(a))).then(a.course_id.cmp(&b.course_id))
    });

    // Revision days: the 2 days before each exam; the earlier exam keeps a contested day.
    let mut reserved: BTreeMap<NaiveDate, (i64, bool)> = BTreeMap::new(); // date -> (course, is mock day)
    for c in &order {
        if let Some(e) = exam_day(c) {
            for (back, mock_day) in [(2, true), (1, false)] {
                let d = e - Duration::days(back);
                if d >= today {
                    reserved.entry(d).or_insert((c.course_id, mock_day));
                }
            }
        }
    }

    let mut left: HashMap<i64, Vec<(PlanLecture, bool)>> = order
        .iter()
        .map(|c| (c.course_id, c.lectures.iter().filter(|l| l.remaining_hours > 0.0).map(|l| (l.clone(), false)).collect()))
        .collect();
    let mut late: HashMap<i64, f64> = HashMap::new();
    let mut entries = Vec::new();

    let mut day = today;
    while day <= horizon_end {
        let mut start = local(tz, day, input.day_start);
        if day == today {
            start = start.max(round_up_15(input.now));
        }
        let end = local(tz, day, input.day_end);
        let busy_today: f64 =
            input.busy.iter().filter(|(s, e)| *s < end && *e > start).map(|(s, e)| hours((*e).min(end) - (*s).max(start))).sum();
        let mut capacity = (input.daily_hours - busy_today).min(hours(end - start).max(0.0));
        let mut cursor = DayCursor { at: start, end, busy: &input.busy };

        let mut place =
            |course: i64, comp: Option<i64>, kind: EntryKind, desc: String, want: f64, capacity: &mut f64, out: &mut Vec<Entry>| -> f64 {
                let want = want.min(*capacity);
                if want < MIN_BLOCK_HOURS - 1e-9 {
                    return 0.0;
                }
                match cursor.take(want) {
                    Some((s, e)) => {
                        let got = hours(e - s);
                        *capacity -= got;
                        out.push(Entry { course_id: course, component_id: comp, kind, description: desc, start: s, end: e });
                        got
                    }
                    None => {
                        *capacity = 0.0;
                        0.0
                    }
                }
            };

        if let Some(&(course, mock_day)) = reserved.get(&day) {
            let c = order.iter().find(|c| c.course_id == course).expect("reserved course");
            if mock_day {
                let mins = c.exam_minutes.map(i64::from).unwrap_or(DEFAULT_MOCK_MINUTES);
                place(course, None, EntryKind::Mock, format!("{}: timed mock", c.code), mins as f64 / 60.0, &mut capacity, &mut entries);
            }
            place(
                course,
                None,
                EntryKind::Revision,
                format!("{}: revision (weak spots, slips, flashcards)", c.code),
                capacity,
                &mut capacity,
                &mut entries,
            );
        } else {
            // Courses still eligible for lecture work today: their revision days haven't started.
            let eligible: Vec<&&PlanCourse> = order
                .iter()
                .filter(|c| exam_day(c).is_none_or(|e| day < e - Duration::days(2)))
                .filter(|c| left.get(&c.course_id).is_some_and(|l| !l.is_empty()))
                .collect();
            let mut used_course = None;
            for c in eligible {
                if used_course.is_some() && !input.multi_course {
                    break;
                }
                let queue = left.get_mut(&c.course_id).expect("course queue");
                while capacity >= MIN_BLOCK_HOURS - 1e-9 {
                    let Some((lec, started)) = queue.first_mut() else { break };
                    let desc = format!("{} {} {}{}", c.code, lec.code, lec.name, if *started { " (cont.)" } else { "" });
                    let got = place(
                        c.course_id,
                        Some(lec.component_id),
                        EntryKind::Lecture,
                        desc,
                        lec.remaining_hours,
                        &mut capacity,
                        &mut entries,
                    );
                    if got <= 0.0 {
                        break;
                    }
                    used_course = Some(c.course_id);
                    if exam_day(c).is_some_and(|e| day >= e - Duration::days(FINAL_WEEK_DAYS)) {
                        *late.entry(c.course_id).or_default() += got;
                    }
                    lec.remaining_hours -= got;
                    *started = true;
                    if lec.remaining_hours < MIN_BLOCK_HOURS / 2.0 {
                        queue.remove(0);
                    }
                }
            }
            // Spare time in a course's final week goes to its revision.
            let revise = match used_course {
                Some(id) => order.iter().find(|c| c.course_id == id).copied(),
                None => order.iter().copied().find(|c| exam_day(c).is_some_and(|e| day >= e - Duration::days(FINAL_WEEK_DAYS) && day < e)),
            };
            if let Some(c) = revise.filter(|c| exam_day(c).is_some_and(|e| day >= e - Duration::days(FINAL_WEEK_DAYS))) {
                place(
                    c.course_id,
                    None,
                    EntryKind::Revision,
                    format!("{}: revision (weak spots, slips, flashcards)", c.code),
                    capacity,
                    &mut capacity,
                    &mut entries,
                );
            }
        }
        day += Duration::days(1);
    }

    for c in &order {
        let unplaced: f64 = left.get(&c.course_id).map(|q| q.iter().map(|(l, _)| l.remaining_hours).sum()).unwrap_or(0.0);
        let total = unplaced + late.get(&c.course_id).copied().unwrap_or(0.0);
        if total >= MIN_BLOCK_HOURS / 2.0 && c.exam.is_some() {
            late.insert(c.course_id, total);
        } else {
            late.remove(&c.course_id);
        }
    }
    let mut short: Vec<(i64, f64)> = late.into_iter().map(|(c, h)| (c, (h * 10.0).round() / 10.0)).collect();
    short.sort_by_key(|(c, _)| *c);
    entries.sort_by_key(|e| e.start);
    Plan { entries, short }
}

/// One line for the rebuild response (spec 5.8 `changeSummary`). `before` and the new entries are
/// compared on (course, lecture, kind, local date).
pub fn change_summary(
    before: &[(i64, Option<i64>, EntryKind, DateTime<Utc>)],
    plan: &Plan,
    pinned_kept: usize,
    tz: Tz,
    codes: &HashMap<i64, String>,
) -> String {
    let key = |c: i64, k: Option<i64>, kind: EntryKind, t: DateTime<Utc>| (c, k, kind, t.with_timezone(&tz).date_naive());
    let mut old: Vec<_> = before.iter().map(|(c, k, kind, t)| key(*c, *k, *kind, *t)).collect();
    let mut new: Vec<_> = plan.entries.iter().map(|e| key(e.course_id, e.component_id, e.kind, e.start)).collect();
    old.sort();
    new.sort();
    let mut parts = Vec::new();
    if old == new {
        parts.push("No changes".to_string());
    } else if let Some(last) = plan.entries.iter().map(|e| e.start).max() {
        let moved = new.iter().filter(|k| !old.contains(k)).count();
        parts.push(format!(
            "{} sessions planned through {}{}",
            plan.entries.len(),
            last.with_timezone(&tz).format("%a %-d %b"),
            if old.is_empty() { String::new() } else { format!(" ({moved} new or moved)") }
        ));
    } else {
        parts.push("Nothing left to plan".to_string());
    }
    if pinned_kept > 0 {
        parts.push(format!("{pinned_kept} pinned kept"));
    }
    for (c, h) in &plan.short {
        parts.push(format!("{} is {h} h short before its final week", codes.get(c).map(String::as_str).unwrap_or("a course")));
    }
    parts.join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const AMS: Tz = chrono_tz::Europe::Amsterdam;

    fn t(day: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, day, h, 0, 0).unwrap()
    }

    fn lec(id: i64, hours: f64) -> PlanLecture {
        PlanLecture { component_id: id, code: format!("L{id}"), name: format!("Lecture {id}"), remaining_hours: hours }
    }

    fn input(courses: Vec<PlanCourse>) -> PlanInput {
        PlanInput {
            now: t(2, 7),
            tz: AMS,
            daily_hours: 4.0,
            multi_course: false,
            day_start: DAY_START,
            day_end: DAY_END,
            courses,
            busy: vec![],
        }
    }

    fn course(id: i64, code: &str, exam: Option<DateTime<Utc>>, lectures: Vec<PlanLecture>) -> PlanCourse {
        PlanCourse { course_id: id, code: code.into(), exam, exam_minutes: Some(90), lectures }
    }

    #[test]
    fn lectures_fill_days_then_revision_and_mock_before_the_exam() {
        // Exam Tue 17 Nov 09:00 Amsterdam (08:00 UTC).
        let p = build(&input(vec![course(1, "BSA", Some(t(17, 8)), vec![lec(1, 3.0), lec(2, 3.0)])]));
        let first = &p.entries[0];
        assert_eq!((first.kind, first.component_id, first.start), (EntryKind::Lecture, Some(1), t(2, 8)), "09:00 local on day one");
        assert_eq!(first.end, t(2, 11));
        assert_eq!(p.entries[1].description, "BSA L2 Lecture 2", "1 h of L2 fits the same day");
        assert_eq!(p.entries[2].description, "BSA L2 Lecture 2 (cont.)");
        let mock = p.entries.iter().find(|e| e.kind == EntryKind::Mock).unwrap();
        assert_eq!(mock.start.with_timezone(&AMS).date_naive(), NaiveDate::from_ymd_opt(2026, 11, 15).unwrap(), "2 days before");
        assert_eq!(mock.end - mock.start, Duration::minutes(90));
        let last = p.entries.last().unwrap();
        assert_eq!(
            (last.kind, last.start.with_timezone(&AMS).date_naive()),
            (EntryKind::Revision, NaiveDate::from_ymd_opt(2026, 11, 16).unwrap())
        );
        assert!(p.short.is_empty());
        // Days per day never exceed capacity.
        let mut per_day: BTreeMap<NaiveDate, f64> = BTreeMap::new();
        for e in &p.entries {
            *per_day.entry(e.start.with_timezone(&AMS).date_naive()).or_default() += hours(e.end - e.start);
        }
        assert!(per_day.values().all(|h| *h <= 4.0 + 1e-9), "{per_day:?}");
    }

    #[test]
    fn one_course_per_day_and_earlier_exam_first() {
        let p =
            build(&input(vec![course(1, "LATE", Some(t(28, 8)), vec![lec(1, 2.0)]), course(2, "SOON", Some(t(10, 8)), vec![lec(2, 2.0)])]));
        assert_eq!(p.entries[0].course_id, 2, "nearest exam first");
        let day_of = |e: &Entry| e.start.with_timezone(&AMS).date_naive();
        let mut by_day: BTreeMap<NaiveDate, Vec<i64>> = BTreeMap::new();
        for e in &p.entries {
            by_day.entry(day_of(e)).or_default().push(e.course_id);
        }
        assert!(by_day.values().all(|cs| cs.iter().all(|c| *c == cs[0])), "one course per day: {by_day:?}");
        let mut multi = input(vec![course(1, "A", None, vec![lec(1, 2.0)]), course(2, "B", None, vec![lec(2, 2.0)])]);
        multi.multi_course = true;
        let p = build(&multi);
        assert_eq!(p.entries.iter().filter(|e| day_of(e) == day_of(&p.entries[0])).count(), 2, "both on day one");
    }

    #[test]
    fn the_users_study_window_bounds_every_block() {
        // 18:00-20:00 local: 2 h a day though capacity is 4 h.
        let mut i = input(vec![course(1, "BSA", None, vec![lec(1, 5.0)])]);
        i.day_start = (18, 0);
        i.day_end = (20, 0);
        let p = build(&i);
        assert_eq!((p.entries[0].start, p.entries[0].end), (t(2, 17), t(2, 19)), "18:00-20:00 Amsterdam");
        for e in &p.entries {
            let (s, end) = (e.start.with_timezone(&AMS).time(), e.end.with_timezone(&AMS).time());
            assert!(s >= NaiveTime::from_hms_opt(18, 0, 0).unwrap() && end <= NaiveTime::from_hms_opt(20, 0, 0).unwrap(), "{e:?}");
        }
        assert_eq!(p.entries.len(), 3, "5 h over three evenings");
        assert_eq!(parse_hhmm("08:45"), Some((8, 45)));
        assert_eq!(parse_hhmm("8:45"), None);
        assert_eq!(parse_hhmm("08:40"), None);
    }

    #[test]
    fn busy_time_is_skipped_and_reduces_capacity() {
        let mut i = input(vec![course(1, "BSA", None, vec![lec(1, 6.0)])]);
        i.busy = vec![(t(2, 9), t(2, 11))]; // 10:00-12:00 local
        let p = build(&i);
        let day1: Vec<_> = p.entries.iter().filter(|e| e.start < t(3, 0)).collect();
        assert_eq!((day1[0].start, day1[0].end), (t(2, 8), t(2, 9)));
        assert_eq!((day1[1].start, day1[1].end), (t(2, 11), t(2, 12)), "2 h capacity left after the pinned 2 h");
    }

    #[test]
    fn too_much_work_is_reported_short() {
        let p = build(&input(vec![course(1, "BSA", Some(t(6, 8)), vec![lec(1, 20.0)])]));
        assert_eq!(p.short.len(), 1);
        assert!(p.short[0].1 > 10.0, "{:?}", p.short);
        let codes = HashMap::from([(1, "BSA".to_string())]);
        let s = change_summary(&[], &p, 1, AMS, &codes);
        assert!(s.contains("sessions planned through") && s.contains("1 pinned kept") && s.contains("BSA is"), "{s}");
        let again: Vec<_> = p.entries.iter().map(|e| (e.course_id, e.component_id, e.kind, e.start)).collect();
        assert!(change_summary(&again, &p, 0, AMS, &codes).starts_with("No changes"));
    }

    #[test]
    fn late_in_the_day_starts_from_now() {
        let mut i = input(vec![course(1, "BSA", None, vec![lec(1, 1.0)])]);
        i.now = Utc.with_ymd_and_hms(2026, 11, 2, 13, 7, 0).unwrap();
        let p = build(&i);
        assert_eq!(p.entries[0].start, Utc.with_ymd_and_hms(2026, 11, 2, 13, 15, 0).unwrap(), "rounded up to the quarter");
    }
}
