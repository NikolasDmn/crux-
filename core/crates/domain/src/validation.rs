use serde::Serialize;

/// One problem with one input field; maps onto `validation_failed.details` (spec 3.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldError {
    pub field: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ValidationErrors(pub Vec<FieldError>);

impl ValidationErrors {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn single(field: &str, code: &str, message: &str) -> Self {
        let mut e = Self::new();
        e.push(field, code, message);
        e
    }

    pub fn push(&mut self, field: &str, code: &str, message: &str) {
        self.0.push(FieldError { field: field.into(), code: code.into(), message: message.into() });
    }

    pub fn merge(&mut self, other: ValidationErrors) {
        self.0.extend(other.0);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `Ok(())` when nothing was pushed, so callers can end with `errors.into_result()?`.
    pub fn into_result(self) -> Result<(), ValidationErrors> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(self)
        }
    }
}

impl std::fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let fields: Vec<&str> = self.0.iter().map(|e| e.field.as_str()).collect();
        write!(f, "invalid fields: {}", fields.join(", "))
    }
}

impl std::error::Error for ValidationErrors {}
