//! Storage saver (spec 6.2, 15): free up space from originals, with a 7-day undo.

use axum::extract::{Path, State};
use axum::Json;
use crux_service::catalog::MaterialView;
use crux_service::saver::{self, FreeSpaceResult};

use crate::error::{ApiResult, ErrorBody};
use crate::extract::Auth;
use crate::AppState;

/// Remove the material's original PDF in 7 days (course Owner only). Text, question images and
/// thumbnails stay.
#[utoipa::path(post, path = "/materials/{id}/free-space", tag = "storage", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = MaterialView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn free_material(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<MaterialView>> {
    Ok(Json(saver::free_material(&s.ctx, &a, &id).await?))
}

/// Undo, before the 7 days are up.
#[utoipa::path(post, path = "/materials/{id}/free-space/cancel", tag = "storage", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = MaterialView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn cancel_material(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<MaterialView>> {
    Ok(Json(saver::cancel_material(&s.ctx, &a, &id).await?))
}

/// Every material with a file, and the past-paper files, of the course (Owner only).
#[utoipa::path(post, path = "/courses/{id}/free-space", tag = "storage", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = FreeSpaceResult), (status = 404, body = ErrorBody)))]
pub async fn free_course(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<FreeSpaceResult>> {
    Ok(Json(saver::free_course(&s.ctx, &a, &id).await?))
}
