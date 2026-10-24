//! Core → the other services for account export (spec 11): each returns its JSON part; files zips
//! the parts with the user's originals.

use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::ClientError;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Built {
    pub file_ref: String,
    pub size_bytes: i64,
}

#[async_trait]
pub trait ExportApi: Send + Sync {
    /// `service` = ai or notify: `POST /internal/v1/export {userRef}` on that service.
    async fn part(&self, service: &str, service_token: &str, user_ref: &str) -> Result<Value, ClientError>;
    /// files: `POST /internal/v1/exports {userRef, exportRef, parts}` → the zip.
    async fn build(&self, service_token: &str, user_ref: &str, export_ref: &str, parts: &Value) -> Result<Built, ClientError>;
}

pub struct HttpExport {
    files: String,
    ai: String,
    notify: String,
    http: reqwest::Client,
}

impl HttpExport {
    pub fn new(files: &str, ai: &str, notify: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(600))
            .connect_timeout(Duration::from_secs(2))
            .build()
            .expect("http client");
        Self {
            files: files.trim_end_matches('/').into(),
            ai: ai.trim_end_matches('/').into(),
            notify: notify.trim_end_matches('/').into(),
            http,
        }
    }
}

#[async_trait]
impl ExportApi for HttpExport {
    async fn part(&self, service: &str, service_token: &str, user_ref: &str) -> Result<Value, ClientError> {
        let base = if service == "ai" { &self.ai } else { &self.notify };
        let resp = self
            .http
            .post(format!("{base}/internal/v1/export"))
            .bearer_auth(service_token)
            .header("x-on-behalf-of", user_ref)
            .json(&json!({ "userRef": user_ref }))
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    async fn build(&self, service_token: &str, user_ref: &str, export_ref: &str, parts: &Value) -> Result<Built, ClientError> {
        let resp = self
            .http
            .post(format!("{}/internal/v1/exports", self.files))
            .bearer_auth(service_token)
            .header("x-on-behalf-of", user_ref)
            .json(&json!({ "userRef": user_ref, "exportRef": export_ref, "parts": parts }))
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }
}

/// Test double: records the parts it was given; ai can be made unreachable.
#[derive(Default)]
pub struct MemoryExport {
    pub built: Mutex<Vec<(String, Value)>>,
    pub ai_down: Mutex<bool>,
}

#[async_trait]
impl ExportApi for MemoryExport {
    async fn part(&self, service: &str, _service_token: &str, user_ref: &str) -> Result<Value, ClientError> {
        if service == "ai" && *self.ai_down.lock().unwrap() {
            return Err(ClientError::Other("ai unreachable".into()));
        }
        Ok(json!({ "service": service, "userRef": user_ref }))
    }

    async fn build(&self, _service_token: &str, _user_ref: &str, export_ref: &str, parts: &Value) -> Result<Built, ClientError> {
        self.built.lock().unwrap().push((export_ref.into(), parts.clone()));
        Ok(Built { file_ref: format!("01EXPORTFILE{:014}", self.built.lock().unwrap().len()), size_bytes: 1234 })
    }
}
