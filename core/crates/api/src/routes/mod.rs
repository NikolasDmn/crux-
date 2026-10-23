pub mod admin;
pub mod auth;
pub mod catalog;
pub mod courses;
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
        .routes(routes!(courses::list, courses::create))
        .routes(routes!(courses::get, courses::patch, courses::trash))
        .routes(routes!(courses::restore))
        .routes(routes!(courses::members, courses::share))
        .routes(routes!(courses::remove_member))
        .routes(routes!(courses::subscribe, courses::unsubscribe))
        .routes(routes!(courses::invitations))
        .routes(routes!(courses::accept))
        .routes(routes!(courses::decline))
        .routes(routes!(courses::grade_scales, courses::create_grade_scale))
        .routes(routes!(catalog::list_components, catalog::create_component))
        .routes(routes!(catalog::patch_component, catalog::archive_component))
        .routes(routes!(catalog::list_concepts, catalog::create_concept))
        .routes(routes!(catalog::list_materials, catalog::create_material))
        .routes(routes!(catalog::patch_material, catalog::archive_material))
        .routes(routes!(catalog::list_papers, catalog::create_paper))
        .routes(routes!(catalog::patch_paper))
}

/// `?limit=&cursor=` (spec 3.1).
#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageQuery {
    /// 1-200, default 50.
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}
