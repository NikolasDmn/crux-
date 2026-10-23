use std::collections::HashSet;
use std::sync::Arc;

use crux_clients::clock::Clock;
use crux_clients::files::FilesApi;
use crux_clients::ids::IdGen;
use crux_clients::kv::KeyValue;
use crux_domain::user::Role;
use crux_repo::MySqlPool;

use crate::jwt::JwtKeys;

#[derive(Debug, Clone)]
pub struct Config {
    /// Public origin of the SPA, for links in emails (`https://crux.example`).
    pub app_base_url: String,
    /// Shared bootstrap secret other services present to get a service token (OPEN-QUESTIONS.md).
    pub service_secret: String,
}

/// Everything a use case needs. Cheap to clone.
#[derive(Clone)]
pub struct Ctx {
    pub pool: MySqlPool,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdGen>,
    pub kv: Arc<dyn KeyValue>,
    pub keys: Arc<JwtKeys>,
    pub files: Arc<dyn FilesApi>,
    pub breached_passwords: Arc<HashSet<String>>,
    pub config: Arc<Config>,
}

/// The signed-in user making the request, resolved from the access token.
#[derive(Debug, Clone)]
pub struct Actor {
    pub user_id: i64,
    pub public_id: String,
    pub role: Role,
    pub email_verified: bool,
}

impl Actor {
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }
}

/// Loads the breached-password list: one password per line, compared lowercase (spec 4.1).
pub fn load_breached_passwords(text: &str) -> HashSet<String> {
    text.lines().map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty() && !l.starts_with('#')).collect()
}
