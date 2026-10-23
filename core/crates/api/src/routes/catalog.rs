//! Lectures, concepts, materials, papers.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use crux_domain::catalog::{ComponentFields, MaterialFields, PaperFields};
use crux_service::catalog::{self, ComponentView, ConceptView, MaterialView, PaperView};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::error::{ApiResult, ErrorBody};
use crate::extract::{Auth, IfMatch, JsonBody};
use crate::AppState;

#[utoipa::path(get, path = "/courses/{id}/components", tag = "lectures", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = Vec<ComponentView>), (status = 404, body = ErrorBody)))]
pub async fn list_components(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<Vec<ComponentView>>> {
    Ok(Json(catalog::list_components(&s.ctx, &a, &id).await?))
}

/// Add a lecture. Prerequisites must be lectures of the same course; cycles are rejected.
#[utoipa::path(post, path = "/courses/{id}/components", tag = "lectures", security(("bearer" = [])), request_body = ComponentFields,
    params(("id" = String, Path), ("Idempotency-Key" = Option<String>, Header)),
    responses((status = 201, body = ComponentView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create_component(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(f): JsonBody<ComponentFields>,
) -> ApiResult<(StatusCode, Json<ComponentView>)> {
    Ok((StatusCode::CREATED, Json(catalog::create_component(&s.ctx, &a, &id, f).await?)))
}

#[utoipa::path(patch, path = "/components/{id}", tag = "lectures", security(("bearer" = [])), request_body = ComponentFields,
    params(("id" = String, Path), ("If-Match" = i32, Header)),
    responses((status = 200, body = ComponentView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn patch_component(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    IfMatch(v): IfMatch,
    JsonBody(f): JsonBody<ComponentFields>,
) -> ApiResult<Json<ComponentView>> {
    Ok(Json(catalog::patch_component(&s.ctx, &a, &id, v, f).await?))
}

/// Archive a lecture (lectures are never hard-deleted).
#[utoipa::path(delete, path = "/components/{id}", tag = "lectures", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorBody)))]
pub async fn archive_component(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<StatusCode> {
    catalog::archive_component(&s.ctx, &a, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/courses/{id}/concepts", tag = "lectures", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = Vec<ConceptView>), (status = 404, body = ErrorBody)))]
pub async fn list_concepts(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<Vec<ConceptView>>> {
    Ok(Json(catalog::list_concepts(&s.ctx, &a, &id).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct ConceptInput {
    pub name: String,
}

#[utoipa::path(post, path = "/courses/{id}/concepts", tag = "lectures", security(("bearer" = [])), request_body = ConceptInput,
    params(("id" = String, Path)), responses((status = 201, body = ConceptView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create_concept(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(i): JsonBody<ConceptInput>,
) -> ApiResult<(StatusCode, Json<ConceptView>)> {
    Ok((StatusCode::CREATED, Json(catalog::create_concept(&s.ctx, &a, &id, &i.name).await?)))
}

#[utoipa::path(get, path = "/components/{id}/materials", tag = "materials", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = Vec<MaterialView>), (status = 404, body = ErrorBody)))]
pub async fn list_materials(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<Vec<MaterialView>>> {
    Ok(Json(catalog::list_materials(&s.ctx, &a, &id).await?))
}

/// Add a material: markdown `content`, or a `fileId` from `/files/uploads` that has been scanned Clean.
#[utoipa::path(post, path = "/components/{id}/materials", tag = "materials", security(("bearer" = [])), request_body = MaterialFields,
    params(("id" = String, Path), ("Idempotency-Key" = Option<String>, Header)),
    responses((status = 201, body = MaterialView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create_material(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(f): JsonBody<MaterialFields>,
) -> ApiResult<(StatusCode, Json<MaterialView>)> {
    Ok((StatusCode::CREATED, Json(catalog::create_material(&s.ctx, &a, &id, f).await?)))
}

#[utoipa::path(patch, path = "/materials/{id}", tag = "materials", security(("bearer" = [])), request_body = MaterialFields,
    params(("id" = String, Path), ("If-Match" = i32, Header)),
    responses((status = 200, body = MaterialView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn patch_material(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    IfMatch(v): IfMatch,
    JsonBody(f): JsonBody<MaterialFields>,
) -> ApiResult<Json<MaterialView>> {
    Ok(Json(catalog::patch_material(&s.ctx, &a, &id, v, f).await?))
}

#[utoipa::path(delete, path = "/materials/{id}", tag = "materials", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorBody)))]
pub async fn archive_material(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<StatusCode> {
    catalog::archive_material(&s.ctx, &a, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Past papers with their exam map (what was left out and why).
#[utoipa::path(get, path = "/courses/{id}/papers", tag = "papers", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = Vec<PaperView>), (status = 404, body = ErrorBody)))]
pub async fn list_papers(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<Vec<PaperView>>> {
    Ok(Json(catalog::list_papers(&s.ctx, &a, &id).await?))
}

#[utoipa::path(post, path = "/courses/{id}/papers", tag = "papers", security(("bearer" = [])), request_body = PaperFields,
    params(("id" = String, Path), ("Idempotency-Key" = Option<String>, Header)),
    responses((status = 201, body = PaperView), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn create_paper(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(f): JsonBody<PaperFields>,
) -> ApiResult<(StatusCode, Json<PaperView>)> {
    Ok((StatusCode::CREATED, Json(catalog::create_paper(&s.ctx, &a, &id, f).await?)))
}

/// `omissions`, when sent, replaces the whole exam map.
#[utoipa::path(patch, path = "/papers/{id}", tag = "papers", security(("bearer" = [])), request_body = PaperFields,
    params(("id" = String, Path), ("If-Match" = i32, Header)),
    responses((status = 200, body = PaperView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn patch_paper(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    IfMatch(v): IfMatch,
    JsonBody(f): JsonBody<PaperFields>,
) -> ApiResult<Json<PaperView>> {
    Ok(Json(catalog::patch_paper(&s.ctx, &a, &id, v, f).await?))
}
