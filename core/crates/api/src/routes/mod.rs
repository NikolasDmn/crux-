pub mod admin;
pub mod auth;
pub mod health;
pub mod internal;
pub mod invites;
pub mod me;
pub mod sse;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;

/// Every public core route. Registering through `routes!` puts each one in the OpenAPI document,
/// which the policy table test and the cross-user harness walk; a route can't skip them.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(auth::signup))
        .routes(routes!(auth::login))
        .routes(routes!(auth::refresh))
        .routes(routes!(auth::logout))
        .routes(routes!(auth::request_verify_email))
        .routes(routes!(auth::confirm_verify_email))
        .routes(routes!(auth::forgot_password))
        .routes(routes!(auth::reset_password))
        .routes(routes!(me::get_me, me::patch_me))
        .routes(routes!(me::get_settings, me::patch_settings))
        .routes(routes!(me::get_tier))
        .routes(routes!(invites::create, invites::list))
        .routes(routes!(invites::revoke))
        .routes(routes!(sse::ticket))
        .routes(routes!(admin::list_users))
        .routes(routes!(admin::patch_user))
        .routes(routes!(admin::list_tiers, admin::create_tier))
        .routes(routes!(admin::update_tier))
        .routes(routes!(admin::list_invites, admin::create_invite))
        .routes(routes!(admin::revoke_invite))
}

/// `?limit=&cursor=` (spec 3.1).
#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageQuery {
    /// 1-200, default 50.
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}
