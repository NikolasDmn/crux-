use std::sync::Arc;

use crux_service::Ctx;

/// Where core finds the other services' OpenAPI documents for the merged `/api/v1/openapi.json`.
#[derive(Debug, Clone, Default)]
pub struct Peers {
    /// `(public prefix, internal base URL)`, e.g. `("/files", "http://files:8080")`.
    pub openapi_sources: Vec<(String, String)>,
}

#[derive(Clone)]
pub struct AppState {
    pub ctx: Ctx,
    pub peers: Arc<Peers>,
    pub cookie_secure: bool,
}
