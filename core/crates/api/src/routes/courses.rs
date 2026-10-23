//! Courses, sharing, invitations, subscriptions, grade scales.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use crux_domain::catalog::{CourseFields, GradeScaleFields};
use crux_service::courses::{self, CourseView, GradeScaleView, InvitationView, MembersView, ShareInput, SubscriptionInput};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::error::{ApiResult, ErrorBody};
use crate::extract::{Auth, IfMatch, JsonBody};
use crate::AppState;

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListQuery {
    /// true = my courses in the trash (D2-5) instead of my live courses.
    #[serde(default)]
    pub trashed: bool,
}

/// Courses I own or have accepted a share of.
#[utoipa::path(get, path = "/courses", tag = "courses", security(("bearer" = [])), params(ListQuery),
    responses((status = 200, body = Vec<CourseView>)))]
pub async fn list(State(s): State<AppState>, Auth(a): Auth, Query(q): Query<ListQuery>) -> ApiResult<Json<Vec<CourseView>>> {
    Ok(Json(courses::list(&s.ctx, &a, q.trashed).await?))
}

/// Create a course. Colour defaults to the next palette colour, grade scale to Percent. The owner is
/// subscribed straight away.
#[utoipa::path(post, path = "/courses", tag = "courses", security(("bearer" = [])), request_body = CourseFields,
    params(("Idempotency-Key" = Option<String>, Header)),
    responses((status = 201, body = CourseView), (status = 422, body = ErrorBody)))]
pub async fn create(
    State(s): State<AppState>,
    Auth(a): Auth,
    JsonBody(f): JsonBody<CourseFields>,
) -> ApiResult<(StatusCode, Json<CourseView>)> {
    Ok((StatusCode::CREATED, Json(courses::create(&s.ctx, &a, f).await?)))
}

#[utoipa::path(get, path = "/courses/{id}", tag = "courses", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = CourseView), (status = 404, body = ErrorBody)))]
pub async fn get(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<CourseView>> {
    Ok(Json(courses::get(&s.ctx, &a, &id).await?))
}

/// Editors: name, code, institution, term. Owner also: colour, grade scale.
#[utoipa::path(patch, path = "/courses/{id}", tag = "courses", security(("bearer" = [])), request_body = CourseFields,
    params(("id" = String, Path), ("If-Match" = i32, Header)),
    responses((status = 200, body = CourseView), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn patch(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    IfMatch(v): IfMatch,
    JsonBody(f): JsonBody<CourseFields>,
) -> ApiResult<Json<CourseView>> {
    Ok(Json(courses::patch(&s.ctx, &a, &id, v, f).await?))
}

/// Owner: move to the trash for 30 days (restorable), then it's purged.
#[utoipa::path(delete, path = "/courses/{id}", tag = "courses", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = CourseView), (status = 404, body = ErrorBody)))]
pub async fn trash(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<CourseView>> {
    Ok(Json(courses::trash(&s.ctx, &a, &id).await?))
}

/// Owner: take a course out of the trash.
#[utoipa::path(post, path = "/courses/{id}/restore", tag = "courses", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = CourseView), (status = 404, body = ErrorBody)))]
pub async fn restore(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<CourseView>> {
    Ok(Json(courses::restore(&s.ctx, &a, &id).await?))
}

/// Owner: who the course is shared with, including pending shares and emails without an account.
#[utoipa::path(get, path = "/courses/{id}/members", tag = "sharing", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 200, body = MembersView), (status = 404, body = ErrorBody)))]
pub async fn members(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<MembersView>> {
    Ok(Json(courses::members(&s.ctx, &a, &id).await?))
}

/// Owner: share with a username or email as Editor or Viewer. The invitee accepts first; an email
/// with no account gets an invite link (needs your verified email).
#[utoipa::path(post, path = "/courses/{id}/members", tag = "sharing", security(("bearer" = [])), request_body = ShareInput,
    params(("id" = String, Path)),
    responses((status = 200, body = MembersView), (status = 403, body = ErrorBody, description = "email_unverified"), (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn share(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(i): JsonBody<ShareInput>,
) -> ApiResult<Json<MembersView>> {
    Ok(Json(courses::share(&s.ctx, &a, &id, i).await?))
}

/// Owner removes a member or withdraws an email share; a member removes themselves (leave).
#[utoipa::path(delete, path = "/courses/{id}/members/{memberId}", tag = "sharing", security(("bearer" = [])),
    params(("id" = String, Path), ("memberId" = String, Path, description = "A user id, or a pending email share's id")),
    responses((status = 204), (status = 404, body = ErrorBody)))]
pub async fn remove_member(State(s): State<AppState>, Auth(a): Auth, Path((id, member)): Path<(String, String)>) -> ApiResult<StatusCode> {
    courses::remove_member(&s.ctx, &a, &id, &member).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Subscribe to track progress (or set it to Archived).
#[utoipa::path(post, path = "/courses/{id}/subscription", tag = "courses", security(("bearer" = [])), request_body = SubscriptionInput,
    params(("id" = String, Path)), responses((status = 200, body = CourseView), (status = 404, body = ErrorBody)))]
pub async fn subscribe(
    State(s): State<AppState>,
    Auth(a): Auth,
    Path(id): Path<String>,
    JsonBody(i): JsonBody<SubscriptionInput>,
) -> ApiResult<Json<CourseView>> {
    Ok(Json(courses::subscribe(&s.ctx, &a, &id, i).await?))
}

#[utoipa::path(delete, path = "/courses/{id}/subscription", tag = "courses", security(("bearer" = [])), params(("id" = String, Path)),
    responses((status = 204), (status = 404, body = ErrorBody)))]
pub async fn unsubscribe(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<StatusCode> {
    courses::unsubscribe(&s.ctx, &a, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Courses shared with me that I haven't accepted yet.
#[utoipa::path(get, path = "/course-invitations", tag = "sharing", security(("bearer" = [])),
    responses((status = 200, body = Vec<InvitationView>)))]
pub async fn invitations(State(s): State<AppState>, Auth(a): Auth) -> ApiResult<Json<Vec<InvitationView>>> {
    Ok(Json(courses::invitations(&s.ctx, &a).await?))
}

#[utoipa::path(post, path = "/course-invitations/{courseId}/accept", tag = "sharing", security(("bearer" = [])),
    params(("courseId" = String, Path)), responses((status = 200, body = CourseView), (status = 404, body = ErrorBody)))]
pub async fn accept(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<Json<CourseView>> {
    let course = courses::respond_to_invitation(&s.ctx, &a, &id, true).await?.ok_or(crux_service::AppError::NotFound)?;
    Ok(Json(course))
}

#[utoipa::path(post, path = "/course-invitations/{courseId}/decline", tag = "sharing", security(("bearer" = [])),
    params(("courseId" = String, Path)), responses((status = 204), (status = 404, body = ErrorBody)))]
pub async fn decline(State(s): State<AppState>, Auth(a): Auth, Path(id): Path<String>) -> ApiResult<StatusCode> {
    courses::respond_to_invitation(&s.ctx, &a, &id, false).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Built-in scales plus mine.
#[utoipa::path(get, path = "/grade-scales", tag = "courses", security(("bearer" = [])),
    responses((status = 200, body = Vec<GradeScaleView>)))]
pub async fn grade_scales(State(s): State<AppState>, Auth(a): Auth) -> ApiResult<Json<Vec<GradeScaleView>>> {
    Ok(Json(courses::grade_scales(&s.ctx, &a).await?))
}

#[utoipa::path(post, path = "/grade-scales", tag = "courses", security(("bearer" = [])), request_body = GradeScaleFields,
    responses((status = 201, body = GradeScaleView), (status = 422, body = ErrorBody)))]
pub async fn create_grade_scale(
    State(s): State<AppState>,
    Auth(a): Auth,
    JsonBody(f): JsonBody<GradeScaleFields>,
) -> ApiResult<(StatusCode, Json<GradeScaleView>)> {
    Ok((StatusCode::CREATED, Json(courses::create_grade_scale(&s.ctx, &a, f).await?)))
}
