//! Core → ai internal API (spec 7.1): the gate. Calls carry a service token (aud = ai). When ai
//! is unreachable (the "ai" profile off, spec 9.6) the caller answers `service_unavailable`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use crate::ClientError;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Allowed {
    pub allowed: bool,
    pub reason: Option<String>,
}

/// A grading suggestion as ai stores it (spec 7.5). Core owns the grade; this is only advice.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradeSuggestion {
    pub id: String,
    pub order: i32,
    pub outcome: String,
    pub grade: f64,
    pub max_grade: f64,
    pub status: String,
}

#[async_trait]
pub trait AiApi: Send + Sync {
    /// The user's suggestions for a mock (`GET /internal/v1/grades?mock=&user=`).
    async fn grades(&self, service_token: &str, user_ref: &str, mock_ref: &str) -> Result<Vec<GradeSuggestion>, ClientError>;
    async fn allowed(
        &self,
        service_token: &str,
        user_ref: &str,
        feature: &str,
        course_ref: Option<&str>,
        materials: &[String],
    ) -> Result<Allowed, ClientError>;
}

pub struct HttpAi {
    base: String,
    http: reqwest::Client,
}

impl HttpAi {
    pub fn new(base: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .connect_timeout(Duration::from_secs(1))
            .build()
            .expect("http client");
        Self { base: base.trim_end_matches('/').to_string(), http }
    }
}

#[async_trait]
impl AiApi for HttpAi {
    async fn grades(&self, service_token: &str, user_ref: &str, mock_ref: &str) -> Result<Vec<GradeSuggestion>, ClientError> {
        let resp = self
            .http
            .get(format!("{}/internal/v1/grades", self.base))
            .bearer_auth(service_token)
            .header("x-on-behalf-of", user_ref)
            .query(&[("mock", mock_ref), ("user", user_ref)])
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    async fn allowed(
        &self,
        service_token: &str,
        user_ref: &str,
        feature: &str,
        course_ref: Option<&str>,
        materials: &[String],
    ) -> Result<Allowed, ClientError> {
        let mut query = vec![("user", user_ref.to_string()), ("feature", feature.to_string()), ("materials", materials.join(","))];
        if let Some(c) = course_ref {
            query.push(("course", c.to_string()));
        }
        let resp = self
            .http
            .get(format!("{}/internal/v1/ai-allowed", self.base))
            .bearer_auth(service_token)
            .header("x-on-behalf-of", user_ref)
            .query(&query)
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }
}

/// Test double: `set(user, feature, reason)`; anything unset answers like a stopped ai service.
#[derive(Default)]
pub struct MemoryAi {
    answers: Mutex<HashMap<(String, String), Option<String>>>,
    suggestions: Mutex<HashMap<String, Vec<GradeSuggestion>>>,
}

impl MemoryAi {
    pub fn set_grades(&self, mock_ref: &str, grades: Vec<GradeSuggestion>) {
        self.suggestions.lock().unwrap().insert(mock_ref.into(), grades);
    }

    pub fn set(&self, user_ref: &str, feature: &str, reason: Option<&str>) {
        self.answers.lock().unwrap().insert((user_ref.into(), feature.into()), reason.map(String::from));
    }
}

#[async_trait]
impl AiApi for MemoryAi {
    async fn grades(&self, _service_token: &str, _user_ref: &str, mock_ref: &str) -> Result<Vec<GradeSuggestion>, ClientError> {
        Ok(self.suggestions.lock().unwrap().get(mock_ref).cloned().unwrap_or_default())
    }

    async fn allowed(
        &self,
        _service_token: &str,
        user_ref: &str,
        feature: &str,
        _course_ref: Option<&str>,
        _materials: &[String],
    ) -> Result<Allowed, ClientError> {
        match self.answers.lock().unwrap().get(&(user_ref.to_string(), feature.to_string())) {
            Some(reason) => Ok(Allowed { allowed: reason.is_none(), reason: reason.clone() }),
            None => Err(ClientError::Other("ai unreachable".into())),
        }
    }
}
