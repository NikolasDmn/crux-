//! Auto-grading and scores (backend-spec 5.4).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Right,
    Partial,
    Wrong,
    Blank,
}

impl Outcome {
    pub fn as_db(self) -> &'static str {
        match self {
            Outcome::Right => "Right",
            Outcome::Partial => "Partial",
            Outcome::Wrong => "Wrong",
            Outcome::Blank => "Blank",
        }
    }

    pub fn from_db(s: &str) -> Option<Self> {
        Some(match s {
            "Right" => Outcome::Right,
            "Partial" => Outcome::Partial,
            "Wrong" => Outcome::Wrong,
            "Blank" => Outcome::Blank,
            _ => return None,
        })
    }

    /// Spec 5.4: Partial counts 0.5.
    pub fn points(self) -> f64 {
        match self {
            Outcome::Right => 1.0,
            Outcome::Partial => 0.5,
            Outcome::Wrong | Outcome::Blank => 0.0,
        }
    }

    /// Wrong, blank or a guess counts as a miss for weights and misses-only (spec 5.3).
    pub fn is_miss(self) -> bool {
        matches!(self, Outcome::Wrong | Outcome::Blank)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Confidence {
    Sure,
    Unsure,
    Guess,
}

impl Confidence {
    pub fn as_db(self) -> &'static str {
        match self {
            Confidence::Sure => "Sure",
            Confidence::Unsure => "Unsure",
            Confidence::Guess => "Guess",
        }
    }

    pub fn from_db(s: &str) -> Option<Self> {
        Some(match s {
            "Sure" => Confidence::Sure,
            "Unsure" => Confidence::Unsure,
            "Guess" => Confidence::Guess,
            _ => return None,
        })
    }
}

/// Unit aliases (spec 5.4: "an alias table, e.g. deg = °"). Keys and values already normalised.
const UNIT_ALIASES: &[(&str, &str)] = &[
    ("deg", "°"),
    ("degree", "°"),
    ("degrees", "°"),
    ("sec", "s"),
    ("secs", "s"),
    ("second", "s"),
    ("seconds", "s"),
    ("min", "min"),
    ("minutes", "min"),
    ("hr", "h"),
    ("hrs", "h"),
    ("hour", "h"),
    ("hours", "h"),
    ("percent", "%"),
    ("pct", "%"),
    ("meter", "m"),
    ("meters", "m"),
    ("metre", "m"),
    ("metres", "m"),
    ("kilogram", "kg"),
    ("kilograms", "kg"),
    ("eur", "€"),
    ("euro", "€"),
    ("euros", "€"),
    ("usd", "$"),
    ("ohm", "ω"),
    ("ohms", "ω"),
    ("m/s^2", "m/s²"),
    ("m/s2", "m/s²"),
    ("m^2", "m²"),
    ("m2", "m²"),
    ("m^3", "m³"),
    ("m3", "m³"),
];

/// Case- and whitespace-insensitive, then aliased.
pub fn normalize_unit(u: &str) -> String {
    let n: String = u.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase();
    UNIT_ALIASES.iter().find(|(k, _)| *k == n).map(|(_, v)| v.to_string()).unwrap_or(n)
}

#[derive(Debug, Clone)]
pub struct NumericRule {
    pub value: f64,
    pub tolerance: f64,
    pub relative: bool,
    pub unit: Option<String>,
    pub unit_required: bool,
}

/// Spec 5.4: Absolute: |a − v| ≤ tol. Relative: |a − v| ≤ tol·|v|. With UnitRequired the unit must
/// match after normalisation. No answer = Blank.
pub fn grade_numeric(rule: &NumericRule, answer: Option<f64>, unit: Option<&str>) -> Outcome {
    let Some(a) = answer.filter(|a| a.is_finite()) else { return Outcome::Blank };
    let allowed = if rule.relative { rule.tolerance * rule.value.abs() } else { rule.tolerance };
    // A hair of float slack so 0.1 + 0.2 style answers on a zero tolerance still match.
    let within = (a - rule.value).abs() <= allowed + f64::EPSILON * rule.value.abs().max(1.0) * 8.0;
    let unit_ok = !rule.unit_required
        || match (&rule.unit, unit) {
            (Some(want), Some(got)) => normalize_unit(want) == normalize_unit(got),
            _ => false,
        };
    if within && unit_ok { Outcome::Right } else { Outcome::Wrong }
}

/// MC: chosen option correct → Right; none chosen → Blank.
pub fn grade_mc(chosen_correct: Option<bool>) -> Outcome {
    match chosen_correct {
        None => Outcome::Blank,
        Some(true) => Outcome::Right,
        Some(false) => Outcome::Wrong,
    }
}

/// An open answer with no text is Blank and needs no grader.
pub fn open_is_blank(text: Option<&str>) -> bool {
    text.is_none_or(|t| t.trim().is_empty())
}

/// Self-grade (spec 5.4): Right / Partial (0.5) / Wrong, scaled to the item's max grade.
pub fn self_grade(outcome: Outcome, max_grade: f64) -> f64 {
    (outcome.points() * max_grade * 100.0).round() / 100.0
}

/// Clamp an AI-suggested grade to [0, max] (spec 7.5).
pub fn clamp_grade(grade: f64, max: f64) -> f64 {
    if grade.is_nan() { 0.0 } else { grade.clamp(0.0, max) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(value: f64, tol: f64, relative: bool) -> NumericRule {
        NumericRule { value, tolerance: tol, relative, unit: None, unit_required: false }
    }

    #[test]
    fn absolute_and_relative_tolerance() {
        assert_eq!(grade_numeric(&rule(10.0, 0.5, false), Some(10.5), None), Outcome::Right);
        assert_eq!(grade_numeric(&rule(10.0, 0.5, false), Some(10.51), None), Outcome::Wrong);
        assert_eq!(grade_numeric(&rule(200.0, 0.01, true), Some(198.0), None), Outcome::Right, "1% of 200 = 2");
        assert_eq!(grade_numeric(&rule(200.0, 0.01, true), Some(197.9), None), Outcome::Wrong);
        assert_eq!(grade_numeric(&rule(-50.0, 0.1, true), Some(-45.0), None), Outcome::Right, "uses |v|");
        assert_eq!(grade_numeric(&rule(0.3, 0.0, false), Some(0.1 + 0.2), None), Outcome::Right, "float noise");
        assert_eq!(grade_numeric(&rule(1.0, 0.0, false), None, None), Outcome::Blank);
        assert_eq!(grade_numeric(&rule(1.0, 0.0, false), Some(f64::NAN), None), Outcome::Blank);
    }

    #[test]
    fn units_normalise_and_alias() {
        let r = NumericRule { value: 90.0, tolerance: 0.0, relative: false, unit: Some("°".into()), unit_required: true };
        assert_eq!(grade_numeric(&r, Some(90.0), Some(" DEG ")), Outcome::Right);
        assert_eq!(grade_numeric(&r, Some(90.0), Some("rad")), Outcome::Wrong);
        assert_eq!(grade_numeric(&r, Some(90.0), None), Outcome::Wrong, "required unit missing");
        let loose = NumericRule { unit_required: false, ..r };
        assert_eq!(grade_numeric(&loose, Some(90.0), None), Outcome::Right);
        assert_eq!(normalize_unit("M / S2"), "m/s²");
    }

    #[test]
    fn mc_open_and_scores() {
        assert_eq!(grade_mc(Some(true)), Outcome::Right);
        assert_eq!(grade_mc(None), Outcome::Blank);
        assert!(open_is_blank(Some("   ")) && !open_is_blank(Some("x")));
        assert_eq!(self_grade(Outcome::Partial, 2.0), 1.0);
        assert_eq!(clamp_grade(7.0, 5.0), 5.0);
        assert_eq!(clamp_grade(-1.0, 5.0), 0.0);
        assert!(Outcome::Blank.is_miss() && !Outcome::Partial.is_miss());
    }
}
