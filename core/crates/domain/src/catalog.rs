//! Courses, lectures (components), materials, papers, grade scales (backend-spec 5.1). Pure rules.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

use crate::ValidationErrors;

/// Spec 5.1: "default palette of 8, assigned round-robin per owner". Placeholder values until the
/// design thread (frontend-spec 3) picks the real ones; mid-saturation so text contrast can be
/// computed either way.
pub const COURSE_PALETTE: [&str; 8] = ["#38bdf8", "#f97316", "#22c55e", "#a855f7", "#ef4444", "#eab308", "#14b8a6", "#ec4899"];

/// D2-6: new courses start on the Percent scale.
pub const DEFAULT_GRADE_SCALE_PUBLIC_ID: &str = "01GRADESCALE0000000PERCENT";

/// D2-5: a deleted course stays in the trash this long, then the purge job removes it.
pub const TRASH_DAYS: i64 = 30;

/// The colour for an owner's next course: round-robin over the palette by how many they own.
pub fn next_color(courses_owned_ever: i64) -> &'static str {
    COURSE_PALETTE[(courses_owned_ever.rem_euclid(COURSE_PALETTE.len() as i64)) as usize]
}

pub fn is_hex_color(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn text(e: &mut ValidationErrors, field: &str, value: &str, max: usize) {
    let n = value.trim().chars().count();
    if n == 0 || n > max {
        e.push(field, "invalid", &format!("Enter 1-{max} characters."));
    }
}

fn opt_text(e: &mut ValidationErrors, field: &str, value: Option<&str>, max: usize) {
    if value.is_some_and(|v| v.chars().count() > max) {
        e.push(field, "invalid", &format!("At most {max} characters."));
    }
}

/// Course codes are short handles used in flashcard codes (`BSA017`): letters and digits only.
pub fn validate_course_code(e: &mut ValidationErrors, code: &str) {
    let ok = (1..=16).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphanumeric());
    if !ok {
        e.push("code", "invalid", "Use 1-16 letters or digits, e.g. BSA.");
    }
}

#[derive(Debug, Clone, Default, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CourseFields {
    pub name: Option<String>,
    pub code: Option<String>,
    pub institution: Option<String>,
    pub term: Option<String>,
    /// Owner only (spec 4.2).
    pub color: Option<String>,
    /// Owner only. A built-in scale or one of yours (`GET /grade-scales`).
    pub grade_scale_id: Option<String>,
}

/// `creating` = POST (name and code required).
pub fn validate_course(f: &CourseFields, creating: bool) -> ValidationErrors {
    let mut e = ValidationErrors::new();
    match (&f.name, creating) {
        (Some(n), _) => text(&mut e, "name", n, 255),
        (None, true) => e.push("name", "required", "Name is required."),
        _ => {}
    }
    match (&f.code, creating) {
        (Some(c), _) => validate_course_code(&mut e, c),
        (None, true) => e.push("code", "required", "Code is required."),
        _ => {}
    }
    opt_text(&mut e, "institution", f.institution.as_deref(), 255);
    opt_text(&mut e, "term", f.term.as_deref(), 32);
    if f.color.as_deref().is_some_and(|c| !is_hex_color(c)) {
        e.push("color", "invalid", "Use a hex colour like #38bdf8.");
    }
    e
}

/// Course fields only the owner may change (spec 4.2: colour, grade scale).
pub fn touches_owner_only_fields(f: &CourseFields) -> bool {
    f.color.is_some() || f.grade_scale_id.is_some()
}

pub fn in_trash_until(archived_at: DateTime<Utc>) -> DateTime<Utc> {
    archived_at + Duration::days(TRASH_DAYS)
}

pub fn purge_due(archived_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    now >= in_trash_until(archived_at)
}

/// Who to share with: an email if it has an `@`, otherwise a username.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShareTarget {
    Email(String),
    Username(String),
}

pub fn share_target(raw: &str) -> ShareTarget {
    let t = raw.trim();
    if t.contains('@') { ShareTarget::Email(crate::auth::normalize_email(t)) } else { ShareTarget::Username(t.to_string()) }
}

// ---------- components (lectures) ----------

#[derive(Debug, Clone, Default, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentFields {
    /// `L1`, `B3`.
    pub code: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub sort_order: Option<i32>,
    pub est_hours: Option<f64>,
    /// Replaces the full prerequisite list. Lectures of the same course only.
    pub prerequisite_ids: Option<Vec<String>>,
}

pub fn validate_component(f: &ComponentFields, creating: bool) -> ValidationErrors {
    let mut e = ValidationErrors::new();
    match (&f.code, creating) {
        (Some(c), _) => {
            if !(1..=8).contains(&c.trim().len()) || !c.trim().bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.') {
                e.push("code", "invalid", "Use 1-8 letters, digits, '-' or '.', e.g. L4.");
            }
        }
        (None, true) => e.push("code", "required", "Code is required."),
        _ => {}
    }
    match (&f.name, creating) {
        (Some(n), _) => text(&mut e, "name", n, 255),
        (None, true) => e.push("name", "required", "Name is required."),
        _ => {}
    }
    opt_text(&mut e, "description", f.description.as_deref(), 20_000);
    if f.est_hours.is_some_and(|h| !(0.0..=999.0).contains(&h)) {
        e.push("estHours", "invalid", "Use 0-999 hours.");
    }
    e
}

/// Does the prerequisite graph have a cycle? `edges` are (prerequisite, dependant). The schema
/// forbids self-loops; this catches longer cycles (review section 6 rule 8).
pub fn has_cycle(edges: &[(i64, i64)]) -> bool {
    let mut graph: HashMap<i64, Vec<i64>> = HashMap::new();
    for &(from, to) in edges {
        graph.entry(from).or_default().push(to);
    }
    // Iterative three-colour DFS.
    let mut state: HashMap<i64, u8> = HashMap::new(); // 1 = on stack, 2 = done
    for &start in graph.keys() {
        if state.get(&start).is_some() {
            continue;
        }
        let mut stack = vec![(start, 0usize)];
        state.insert(start, 1);
        while let Some((node, idx)) = stack.pop() {
            let next = graph.get(&node).and_then(|n| n.get(idx)).copied();
            match next {
                Some(child) => {
                    stack.push((node, idx + 1));
                    match state.get(&child) {
                        Some(1) => return true,
                        Some(_) => {}
                        None => {
                            state.insert(child, 1);
                            stack.push((child, 0));
                        }
                    }
                }
                None => {
                    state.insert(node, 2);
                }
            }
        }
    }
    false
}

/// The course's edges with `component`'s prerequisites replaced by `new_prereqs`.
pub fn replace_prerequisites(existing: &[(i64, i64)], component: i64, new_prereqs: &[i64]) -> Vec<(i64, i64)> {
    let mut out: Vec<(i64, i64)> = existing.iter().copied().filter(|&(_, dep)| dep != component).collect();
    let unique: HashSet<i64> = new_prereqs.iter().copied().collect();
    out.extend(unique.into_iter().map(|p| (p, component)));
    out
}

// ---------- materials ----------

pub const MATERIAL_KINDS: [&str; 6] = ["Slides", "Textbook", "Exercise", "PastExam", "Notes", "Other"];

#[derive(Debug, Clone, Default, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaterialFields {
    /// Slides, Textbook, Exercise, PastExam, Notes, Other.
    pub kind: Option<String>,
    pub title: Option<String>,
    /// Markdown body (or `fileId`).
    pub content: Option<String>,
    /// An uploaded PDF (files service). Must be Active and scanned Clean.
    pub file_id: Option<String>,
    pub page_from: Option<i32>,
    pub page_to: Option<i32>,
    pub excluded: Option<bool>,
    pub excluded_note: Option<String>,
    /// Keep this file away from every AI model (licence, copyright).
    pub ai_allowed: Option<bool>,
    pub sort_order: Option<i32>,
}

pub fn validate_material(f: &MaterialFields, creating: bool, has_existing_source: bool) -> ValidationErrors {
    let mut e = ValidationErrors::new();
    match (&f.kind, creating) {
        (Some(k), _) if !MATERIAL_KINDS.contains(&k.as_str()) => e.push("kind", "invalid", "Unknown kind."),
        (None, true) => e.push("kind", "required", "Kind is required."),
        _ => {}
    }
    match (&f.title, creating) {
        (Some(t), _) => text(&mut e, "title", t, 255),
        (None, true) => e.push("title", "required", "Title is required."),
        _ => {}
    }
    if f.content.is_some() && f.file_id.is_some() {
        e.push("content", "exclusive", "Give either markdown content or a file, not both.");
    }
    if creating && f.content.is_none() && f.file_id.is_none() && !has_existing_source {
        e.push("content", "required", "Add markdown content or a file.");
    }
    if f.content.as_deref().is_some_and(|c| c.len() > 16 * 1024 * 1024) {
        e.push("content", "too_long", "At most 16 MB of text.");
    }
    if f.page_from.is_some_and(|p| p < 1) {
        e.push("pageFrom", "invalid", "Pages start at 1.");
    }
    if let (Some(a), Some(b)) = (f.page_from, f.page_to) {
        if b < a {
            e.push("pageTo", "invalid", "Must be on or after the first page.");
        }
    }
    opt_text(&mut e, "excludedNote", f.excluded_note.as_deref(), 255);
    e
}

// ---------- papers ----------

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Omission {
    pub question_number: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaperFields {
    /// Short unique key within the course, e.g. `2024-06`.
    pub paper_key: Option<String>,
    pub label: Option<String>,
    /// `YYYY-MM-DD`.
    pub exam_date: Option<chrono::NaiveDate>,
    pub file_id: Option<String>,
    pub total_questions: Option<i32>,
    /// Official, Partial, Derived, None.
    pub key_status: Option<String>,
    pub note: Option<String>,
    /// Replaces the full exam map of left-out questions.
    pub omissions: Option<Vec<Omission>>,
}

pub fn validate_paper(f: &PaperFields, creating: bool) -> ValidationErrors {
    let mut e = ValidationErrors::new();
    match (&f.paper_key, creating) {
        (Some(k), _) => text(&mut e, "paperKey", k, 32),
        (None, true) => e.push("paperKey", "required", "Key is required."),
        _ => {}
    }
    match (&f.label, creating) {
        (Some(l), _) => text(&mut e, "label", l, 255),
        (None, true) => e.push("label", "required", "Label is required."),
        _ => {}
    }
    if f.total_questions.is_some_and(|n| !(0..=1000).contains(&n)) {
        e.push("totalQuestions", "invalid", "Use 0-1000.");
    }
    if f.key_status.as_deref().is_some_and(|k| !["Official", "Partial", "Derived", "None"].contains(&k)) {
        e.push("keyStatus", "invalid", "Use Official, Partial, Derived or None.");
    }
    if let Some(om) = &f.omissions {
        let mut seen = HashSet::new();
        for (i, o) in om.iter().enumerate() {
            if o.question_number.trim().is_empty() || o.question_number.len() > 16 || !seen.insert(o.question_number.trim()) {
                e.push(&format!("omissions[{i}].questionNumber"), "invalid", "1-16 characters, unique per paper.");
            }
            if o.reason.trim().is_empty() || o.reason.chars().count() > 255 {
                e.push(&format!("omissions[{i}].reason"), "invalid", "Enter 1-255 characters.");
            }
        }
    }
    e
}

// ---------- grade scales ----------

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GradeLabel {
    pub label: String,
    pub value: f64,
    pub pass: bool,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GradeScaleFields {
    pub name: String,
    /// Numeric, Letter, PassFail.
    pub kind: String,
    pub scale_min: Option<f64>,
    pub scale_max: Option<f64>,
    pub step: Option<f64>,
    pub pass_value: Option<f64>,
    /// Letter / PassFail: the grades in order.
    pub labels: Option<Vec<GradeLabel>>,
}

pub fn validate_grade_scale(f: &GradeScaleFields) -> ValidationErrors {
    let mut e = ValidationErrors::new();
    text(&mut e, "name", &f.name, 64);
    match f.kind.as_str() {
        "Numeric" => match (f.scale_min, f.scale_max) {
            (Some(lo), Some(hi)) if hi > lo && lo >= -9999.0 && hi <= 9999.0 => {
                if f.pass_value.is_some_and(|p| p < lo || p > hi) {
                    e.push("passValue", "invalid", "Must be within the scale.");
                }
                if f.step.is_some_and(|s| s <= 0.0) {
                    e.push("step", "invalid", "Must be more than 0.");
                }
            }
            _ => e.push("scaleMax", "invalid", "Give scaleMin and scaleMax, with max above min."),
        },
        "Letter" | "PassFail" => {
            let labels = f.labels.as_deref().unwrap_or(&[]);
            if labels.len() < 2 || labels.len() > 30 {
                e.push("labels", "invalid", "Give 2-30 grades.");
            } else if !labels.iter().any(|l| l.pass) || labels.iter().all(|l| l.pass) {
                e.push("labels", "invalid", "Mark which grades pass (at least one passes and one fails).");
            } else if labels.iter().any(|l| l.label.trim().is_empty() || l.label.chars().count() > 16) {
                e.push("labels", "invalid", "Each grade needs a 1-16 character label.");
            }
        }
        _ => e.push("kind", "invalid", "Use Numeric, Letter or PassFail."),
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn colours_round_robin_and_validate() {
        assert_eq!(next_color(0), COURSE_PALETTE[0]);
        assert_eq!(next_color(8), COURSE_PALETTE[0]);
        assert_eq!(next_color(9), COURSE_PALETTE[1]);
        assert!(COURSE_PALETTE.iter().all(|c| is_hex_color(c)));
        assert!(!is_hex_color("38bdf8") && !is_hex_color("#38bdfg") && !is_hex_color("#fff"));
    }

    #[test]
    fn course_validation() {
        let ok = CourseFields { name: Some("Business Statistics".into()), code: Some("BSA".into()), ..Default::default() };
        assert!(validate_course(&ok, true).is_empty());
        assert_eq!(validate_course(&CourseFields::default(), true).0.len(), 2);
        assert!(validate_course(&CourseFields::default(), false).is_empty(), "PATCH may change nothing");
        let bad = CourseFields { code: Some("B S".into()), color: Some("red".into()), ..Default::default() };
        let fields: Vec<_> = validate_course(&bad, false).0.into_iter().map(|f| f.field).collect();
        assert_eq!(fields, ["code", "color"]);
        assert!(touches_owner_only_fields(&CourseFields { color: Some("#000000".into()), ..Default::default() }));
        assert!(!touches_owner_only_fields(&ok));
    }

    #[test]
    fn trash_window_is_30_days() {
        let t = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
        assert!(!purge_due(t, t + Duration::days(29)));
        assert!(purge_due(t, t + Duration::days(30)));
    }

    #[test]
    fn share_targets() {
        assert_eq!(share_target(" Nick@Example.com "), ShareTarget::Email("nick@example.com".into()));
        assert_eq!(share_target("nick"), ShareTarget::Username("nick".into()));
    }

    #[test]
    fn cycles() {
        assert!(!has_cycle(&[]));
        assert!(!has_cycle(&[(1, 2), (2, 3), (1, 3)]), "a diamond is fine");
        assert!(has_cycle(&[(1, 2), (2, 1)]));
        assert!(has_cycle(&[(1, 2), (2, 3), (3, 1)]));
        assert!(!has_cycle(&[(1, 2), (3, 4), (4, 5)]));
        let edges = replace_prerequisites(&[(1, 2), (2, 3)], 1, &[3, 3]);
        assert!(has_cycle(&edges), "making L1 depend on L3 closes the loop");
        assert_eq!(edges.iter().filter(|e| **e == (3, 1)).count(), 1, "duplicates collapse");
        let cleared = replace_prerequisites(&[(1, 2), (3, 2)], 2, &[]);
        assert!(cleared.is_empty());
    }

    #[test]
    fn component_validation() {
        let ok = ComponentFields { code: Some("L4".into()), name: Some("Regression".into()), est_hours: Some(3.5), ..Default::default() };
        assert!(validate_component(&ok, true).is_empty());
        let bad = ComponentFields { code: Some("Lecture 4".into()), est_hours: Some(-1.0), ..Default::default() };
        assert_eq!(validate_component(&bad, true).0.len(), 3);
    }

    #[test]
    fn material_validation() {
        let md = MaterialFields { kind: Some("Notes".into()), title: Some("Week 1".into()), content: Some("# Hi".into()), ..Default::default() };
        assert!(validate_material(&md, true, false).is_empty());
        let both = MaterialFields { file_id: Some("01X".into()), ..md.clone() };
        assert_eq!(validate_material(&both, true, false).0[0].code, "exclusive");
        let none = MaterialFields { content: None, ..md.clone() };
        assert_eq!(validate_material(&none, true, false).0[0].code, "required");
        let pages = MaterialFields { page_from: Some(5), page_to: Some(2), kind: Some("Movie".into()), ..md };
        assert_eq!(validate_material(&pages, true, false).0.len(), 2);
    }

    #[test]
    fn paper_validation() {
        let ok = PaperFields {
            paper_key: Some("2024-06".into()),
            label: Some("June 2024".into()),
            key_status: Some("Official".into()),
            omissions: Some(vec![Omission { question_number: "7".into(), reason: "Not in syllabus".into() }]),
            ..Default::default()
        };
        assert!(validate_paper(&ok, true).is_empty());
        let dup = PaperFields {
            omissions: Some(vec![
                Omission { question_number: "7".into(), reason: "a".into() },
                Omission { question_number: "7".into(), reason: "b".into() },
            ]),
            key_status: Some("Maybe".into()),
            ..Default::default()
        };
        assert_eq!(validate_paper(&dup, false).0.len(), 2);
    }

    #[test]
    fn grade_scale_validation() {
        let numeric =
            GradeScaleFields { name: "UK".into(), kind: "Numeric".into(), scale_min: Some(0.0), scale_max: Some(100.0), step: Some(1.0), pass_value: Some(40.0), labels: None };
        assert!(validate_grade_scale(&numeric).is_empty());
        let bad = GradeScaleFields { pass_value: Some(120.0), ..numeric.clone() };
        assert_eq!(validate_grade_scale(&bad).0[0].field, "passValue");
        let letters = GradeScaleFields {
            name: "US".into(),
            kind: "Letter".into(),
            scale_min: None,
            scale_max: None,
            step: None,
            pass_value: None,
            labels: Some(vec![GradeLabel { label: "A".into(), value: 4.0, pass: true }, GradeLabel { label: "F".into(), value: 0.0, pass: false }]),
        };
        assert!(validate_grade_scale(&letters).is_empty());
        let all_pass = GradeScaleFields { labels: Some(vec![GradeLabel { label: "A".into(), value: 4.0, pass: true }; 2]), ..letters };
        assert_eq!(validate_grade_scale(&all_pass).0[0].field, "labels");
    }
}
