//! Exams and deadlines (backend-spec 5.10). Milestone 3 needs only the lookup the mock builder
//! uses; the routes arrive with milestone 5.

use crux_repo::MySqlConnection;

use crate::AppResult;

/// The actor's deadline on this course, by PublicId (deadlines hang off the user's subscription).
pub async fn deadline_id_for(conn: &mut MySqlConnection, user_id: i64, course_id: i64, deadline_ref: &str) -> AppResult<Option<i64>> {
    Ok(crux_repo::mocks::deadline_id_for(conn, user_id, course_id, deadline_ref).await?)
}
