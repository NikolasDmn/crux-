//! Question bank rules (backend-spec 5.2; review section 6 rules 3-6, 10).

use serde::{Deserialize, Serialize};

use crate::ValidationErrors;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub enum QuestionType {
    MC,
    Numeric,
    Open,
}

impl QuestionType {
    pub fn as_db(self) -> &'static str {
        match self {
            QuestionType::MC => "MC",
            QuestionType::Numeric => "Numeric",
            QuestionType::Open => "Open",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "MC" => QuestionType::MC,
            "Numeric" => QuestionType::Numeric,
            _ => QuestionType::Open,
        }
    }
}

pub const SOURCES: [&str; 8] = ["Generated", "Lecture", "Tutorial", "PastExam", "Practice", "Manual", "Imported", "Unknown"];
pub const RELIABILITY: [&str; 3] = ["Official", "Derived", "Disputed"];

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OptionInput {
    pub text: String,
    pub is_correct: bool,
    /// Why a wrong option is tempting (the slip it catches).
    pub distractor_note: Option<String>,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NumericKey {
    pub value: f64,
    #[serde(default)]
    pub tolerance: f64,
    /// Absolute or Relative (default).
    #[serde(default = "relative")]
    pub tolerance_kind: String,
    pub unit: Option<String>,
    #[serde(default)]
    pub unit_required: bool,
    /// Worked solution.
    pub model_answer: Option<String>,
}

fn relative() -> String {
    "Relative".into()
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RubricRow {
    pub criterion: String,
    pub points: f64,
}

#[derive(Debug, Clone, Default, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenKey {
    pub model_answer: Option<String>,
    pub rubric_note: Option<String>,
    pub rubric: Option<Vec<RubricRow>>,
}

/// One version's content. Every edit is a new immutable version (spec 5.2).
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VersionInput {
    /// Markdown with `$…$` maths.
    pub prompt: String,
    pub explanation: Option<String>,
    /// A cropped or uploaded image (files, Purpose QuestionImage).
    pub image_file_id: Option<String>,
    /// Official, Derived (default), Disputed.
    pub key_reliability: Option<String>,
    pub key_note: Option<String>,
    pub change_note: Option<String>,
    /// MC only.
    pub options: Option<Vec<OptionInput>>,
    #[serde(default = "yes")]
    pub shuffle_options: bool,
    /// MC: "choose all that apply" (one or more correct options; answers pick several).
    #[serde(default)]
    pub multi_select: bool,
    /// Numeric only.
    pub numeric: Option<NumericKey>,
    /// Open only.
    pub open: Option<OpenKey>,
}

fn yes() -> bool {
    true
}

/// Review section 6 rules 4-5 and 10: the key matches the type; MC has ≥ 2 options and exactly 1
/// correct; a generated question is never Official.
pub fn validate_version(qt: QuestionType, v: &VersionInput, source: &str) -> ValidationErrors {
    let mut e = ValidationErrors::new();
    let prompt_len = v.prompt.trim().chars().count();
    if prompt_len == 0 || v.prompt.len() > 64 * 1024 {
        e.push("prompt", "invalid", "Write the question (up to 64 KB).");
    }
    if let Some(r) = &v.key_reliability {
        if !RELIABILITY.contains(&r.as_str()) {
            e.push("keyReliability", "invalid", "Use Official, Derived or Disputed.");
        } else if r == "Official" && source == "Generated" {
            e.push("keyReliability", "generated", "A generated question's key can't be Official.");
        }
    }
    match qt {
        QuestionType::MC => {
            let opts = v.options.as_deref().unwrap_or(&[]);
            if !(2..=12).contains(&opts.len()) {
                e.push("options", "count", "Give 2-12 options.");
            } else if v.multi_select && !opts.iter().any(|o| o.is_correct) {
                e.push("options", "correct", "Mark at least one option as correct.");
            } else if !v.multi_select && opts.iter().filter(|o| o.is_correct).count() != 1 {
                e.push("options", "correct", "Mark exactly one option as correct.");
            }
            if opts.iter().any(|o| o.text.trim().is_empty()) {
                e.push("options", "empty", "Options can't be empty.");
            }
            let mut texts: Vec<String> = opts.iter().map(|o| o.text.trim().to_lowercase()).collect();
            texts.sort();
            texts.dedup();
            if texts.len() != opts.len() {
                e.push("options", "duplicate", "Two options are the same.");
            }
            if v.numeric.is_some() || v.open.is_some() {
                e.push("type", "mismatch", "An MC question has options only.");
            }
        }
        QuestionType::Numeric => match &v.numeric {
            Some(n) => {
                if !n.value.is_finite() {
                    e.push("numeric.value", "invalid", "Give a number.");
                }
                if !n.tolerance.is_finite() || n.tolerance < 0.0 {
                    e.push("numeric.tolerance", "invalid", "Tolerance must be 0 or more.");
                }
                if !matches!(n.tolerance_kind.as_str(), "Absolute" | "Relative") {
                    e.push("numeric.toleranceKind", "invalid", "Use Absolute or Relative.");
                }
                if n.unit_required && n.unit.as_deref().is_none_or(|u| u.trim().is_empty()) {
                    e.push("numeric.unit", "required", "A required unit needs a unit.");
                }
                if v.options.is_some() || v.open.is_some() {
                    e.push("type", "mismatch", "A numeric question has a numeric key only.");
                }
            }
            None => e.push("numeric", "required", "Give the answer value."),
        },
        QuestionType::Open => {
            if v.options.is_some() || v.numeric.is_some() {
                e.push("type", "mismatch", "An open question has a model answer / rubric only.");
            }
            if let Some(rows) = v.open.as_ref().and_then(|o| o.rubric.as_ref()) {
                if rows.iter().any(|r| r.criterion.trim().is_empty() || !r.points.is_finite() || r.points < 0.0) {
                    e.push("open.rubric", "invalid", "Each rubric row needs a criterion and points ≥ 0.");
                }
            }
        }
    }
    e
}

/// Review rule 3: `PaperQuestionNo` needs `PaperId`.
pub fn validate_paper_link(paper_id: Option<&str>, number: Option<&str>) -> ValidationErrors {
    if number.is_some() && paper_id.is_none() {
        ValidationErrors::single("paperQuestionNo", "needs_paper", "Pick the paper the number belongs to.")
    } else if number.is_some_and(|n| n.trim().is_empty() || n.len() > 16) {
        ValidationErrors::single("paperQuestionNo", "invalid", "1-16 characters.")
    } else {
        ValidationErrors::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(text: &str, ok: bool) -> OptionInput {
        OptionInput { text: text.into(), is_correct: ok, distractor_note: None }
    }

    fn base() -> VersionInput {
        VersionInput {
            prompt: "What is $2+2$?".into(),
            explanation: None,
            image_file_id: None,
            key_reliability: None,
            key_note: None,
            change_note: None,
            options: None,
            shuffle_options: true,
            multi_select: false,
            numeric: None,
            open: None,
        }
    }

    #[test]
    fn mc_needs_two_options_and_exactly_one_correct() {
        let ok = VersionInput { options: Some(vec![opt("4", true), opt("5", false)]), ..base() };
        assert!(validate_version(QuestionType::MC, &ok, "Manual").is_empty());
        let one = VersionInput { options: Some(vec![opt("4", true)]), ..base() };
        assert_eq!(validate_version(QuestionType::MC, &one, "Manual").0[0].code, "count");
        let two = VersionInput { options: Some(vec![opt("4", true), opt("four", true)]), ..base() };
        assert_eq!(validate_version(QuestionType::MC, &two, "Manual").0[0].code, "correct");
        let dup = VersionInput { options: Some(vec![opt("4", true), opt(" 4 ", false)]), ..base() };
        assert_eq!(validate_version(QuestionType::MC, &dup, "Manual").0[0].code, "duplicate");
    }

    #[test]
    fn keys_must_match_the_type() {
        let numeric_on_mc = VersionInput {
            options: Some(vec![opt("a", true), opt("b", false)]),
            numeric: Some(NumericKey {
                value: 1.0,
                tolerance: 0.0,
                tolerance_kind: "Absolute".into(),
                unit: None,
                unit_required: false,
                model_answer: None,
            }),
            ..base()
        };
        assert!(validate_version(QuestionType::MC, &numeric_on_mc, "Manual").0.iter().any(|f| f.code == "mismatch"));
        assert_eq!(validate_version(QuestionType::Numeric, &base(), "Manual").0[0].field, "numeric");
        assert!(validate_version(QuestionType::Open, &base(), "Manual").is_empty());
    }

    #[test]
    fn numeric_key_rules() {
        let n = |tol: f64, kind: &str, unit_required: bool| VersionInput {
            numeric: Some(NumericKey {
                value: 9.81,
                tolerance: tol,
                tolerance_kind: kind.into(),
                unit: None,
                unit_required,
                model_answer: None,
            }),
            ..base()
        };
        assert!(validate_version(QuestionType::Numeric, &n(0.01, "Relative", false), "Manual").is_empty());
        assert_eq!(validate_version(QuestionType::Numeric, &n(-1.0, "Relative", false), "Manual").0[0].field, "numeric.tolerance");
        assert_eq!(validate_version(QuestionType::Numeric, &n(0.0, "Percent", false), "Manual").0[0].field, "numeric.toleranceKind");
        assert_eq!(validate_version(QuestionType::Numeric, &n(0.0, "Absolute", true), "Manual").0[0].field, "numeric.unit");
    }

    #[test]
    fn generated_is_never_official() {
        let v = VersionInput { key_reliability: Some("Official".into()), ..base() };
        assert_eq!(validate_version(QuestionType::Open, &v, "Generated").0[0].code, "generated");
        assert!(validate_version(QuestionType::Open, &v, "PastExam").is_empty());
    }

    #[test]
    fn paper_number_needs_a_paper() {
        assert!(!validate_paper_link(None, Some("3")).is_empty());
        assert!(validate_paper_link(Some("01P"), Some("3")).is_empty());
        assert!(validate_paper_link(None, None).is_empty());
    }
}
