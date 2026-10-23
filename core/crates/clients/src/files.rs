//! Core → files internal API (`GET /internal/v1/files/{ref}`, spec 15). Calls carry a service token
//! (aud = files) and `X-On-Behalf-Of`; files answers 404 when the file is gone (spec 9.1).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::ClientError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub id: String,
    /// Whose quota it counts against (D2-4: the course owner for course uploads).
    pub owner_ref: String,
    pub uploaded_by_ref: Option<String>,
    pub course_ref: Option<String>,
    pub purpose: String,
    pub status: String,
    pub scan_status: String,
    pub extract_status: String,
    pub sha256: String,
    pub size_bytes: i64,
    pub page_count: Option<i32>,
    pub file_name: String,
}

impl FileInfo {
    /// Spec 5.1: a material may point at a file only when it's Active and scanned Clean.
    pub fn usable_original(&self) -> bool {
        self.purpose == "Original" && self.status == "Active" && self.scan_status == "Clean"
    }
}

#[async_trait]
pub trait FilesApi: Send + Sync {
    /// `None` = no such file (or it was deleted).
    async fn get_file(&self, service_token: &str, on_behalf_of: &str, file_ref: &str) -> Result<Option<FileInfo>, ClientError>;
}

pub struct HttpFiles {
    base: String,
    http: reqwest::Client,
}

impl HttpFiles {
    pub fn new(base: &str) -> Self {
        let http = reqwest::Client::builder().timeout(Duration::from_secs(2)).build().expect("http client");
        Self { base: base.trim_end_matches('/').to_string(), http }
    }
}

#[async_trait]
impl FilesApi for HttpFiles {
    async fn get_file(&self, service_token: &str, on_behalf_of: &str, file_ref: &str) -> Result<Option<FileInfo>, ClientError> {
        // Two tries with a short pause: GET is idempotent (spec 3.2 retry rule).
        let mut last = None;
        for attempt in 0..2 {
            let resp = self
                .http
                .get(format!("{}/internal/v1/files/{}", self.base, file_ref))
                .bearer_auth(service_token)
                .header("x-on-behalf-of", on_behalf_of)
                .send()
                .await;
            match resp {
                Ok(r) if r.status() == reqwest::StatusCode::NOT_FOUND => return Ok(None),
                Ok(r) => return Ok(Some(r.error_for_status()?.json().await?)),
                Err(e) => last = Some(e),
            }
            if attempt == 0 {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        Err(last.expect("an error").into())
    }
}

/// Test double.
#[derive(Default)]
pub struct MemoryFiles {
    files: Mutex<HashMap<String, FileInfo>>,
}

impl MemoryFiles {
    pub fn put(&self, f: FileInfo) {
        self.files.lock().unwrap().insert(f.id.clone(), f);
    }
}

#[async_trait]
impl FilesApi for MemoryFiles {
    async fn get_file(&self, _token: &str, _on_behalf_of: &str, file_ref: &str) -> Result<Option<FileInfo>, ClientError> {
        Ok(self.files.lock().unwrap().get(file_ref).cloned())
    }
}
