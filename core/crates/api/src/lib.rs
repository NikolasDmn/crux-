//! HTTP layer: routes, request/response DTOs, auth extraction (spec 2.1). Routes call services,
//! never repos.

pub mod error;
pub mod extract;
pub mod idempotency;
pub mod metrics;
pub mod openapi;
pub mod policy_table;
pub mod reqlog;
pub mod retry;
pub mod routes;
pub mod state;

pub use state::AppState;

use axum::Router;

/// The whole core HTTP app. Paths are what the gateway forwards after stripping `/api/v1`.
pub fn app(state: AppState) -> Router {
    let (router, api) = routes::router().split_for_parts();
    let api = openapi::finish(api);
    router
        .merge(routes::health::router())
        .merge(routes::internal::router())
        .merge(openapi::router(api))
        .layer(axum::middleware::from_fn(retry::layer))
        .layer(axum::middleware::from_fn_with_state(state.clone(), idempotency::layer))
        .layer(axum::middleware::from_fn(reqlog::layer))
        .with_state(state)
}

/// Core's own OpenAPI document (also what the cross-user harness walks).
pub fn openapi_doc() -> utoipa::openapi::OpenApi {
    openapi::finish(routes::router().split_for_parts().1)
}
