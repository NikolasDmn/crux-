use axum::extract::State;
use axum::Json;
use crux_service::sse::{self, Ticket};

use crate::error::{ApiResult, ErrorBody};
use crate::extract::Auth;
use crate::AppState;

/// A 60-second single-use ticket for opening an EventSource (`?ticket=`).
#[utoipa::path(post, path = "/sse-ticket", tag = "sse", security(("bearer" = [])),
    responses((status = 200, body = Ticket), (status = 401, body = ErrorBody)))]
pub async fn ticket(State(state): State<AppState>, Auth(actor): Auth) -> ApiResult<Json<Ticket>> {
    Ok(Json(sse::issue(&state.ctx, &actor).await?))
}
