//! OpenAPI 3.1 (spec 3.1). Core serves its own at `/openapi.json` and the merged document for the
//! whole API at `/api/v1/openapi.json` (the gateway forwards that path unstripped).

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::openapi::{InfoBuilder, OpenApi, ServerBuilder};

use crate::AppState;

pub fn finish(mut api: OpenApi) -> OpenApi {
    api.info = InfoBuilder::new().title("Crux core").version(env!("CARGO_PKG_VERSION")).build();
    api.servers = Some(vec![ServerBuilder::new().url("/api/v1").build()]);
    let components = api.components.get_or_insert_with(Default::default);
    components.add_security_scheme(
        "bearer",
        SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).bearer_format("JWT").build()),
    );
    api
}

pub fn router(api: OpenApi) -> Router<AppState> {
    let own = api.clone();
    Router::new()
        .route("/openapi.json", get(move || async move { Json(own) }))
        .route("/api/v1/openapi.json", get(move |state: State<AppState>| merged(state, api)))
}

/// Core's paths plus each peer's paths under its prefix. A peer that is down is left out (the ai
/// service may not be deployed at all; spec 1).
async fn merged(State(state): State<AppState>, mut api: OpenApi) -> Json<OpenApi> {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(2)).build().unwrap_or_default();
    for (prefix, base) in &state.peers.openapi_sources {
        let Ok(resp) = client.get(format!("{base}/openapi.json")).send().await else { continue };
        let Ok(peer) = resp.json::<OpenApi>().await else { continue };
        for (path, item) in peer.paths.paths {
            api.paths.paths.insert(format!("{prefix}{path}"), item);
        }
        if let (Some(mine), Some(theirs)) = (api.components.as_mut(), peer.components) {
            mine.schemas.extend(theirs.schemas);
        }
    }
    api.info.title = "Crux API".into();
    Json(api)
}
